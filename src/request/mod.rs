use std::borrow::Cow;
use std::convert::{From, TryInto};
use std::io::{prelude::*, BufWriter};
use std::str;

#[cfg(feature = "compress")]
use http::header::ACCEPT_ENCODING;
use http::{
    header::{HeaderValue, IntoHeaderName, HOST},
    HeaderMap, Method, StatusCode, Version,
};
use url::Url;

use crate::error::{Error, ErrorKind, InvalidResponseKind, Result};
use crate::parsing::{parse_response, Response};
use crate::streams::{BaseStream, ConnectInfo};

mod builder;
mod session;
mod settings;

pub use builder::RequestBuilder;
pub use session::Session;
pub use settings::BaseSettings;

fn header_insert<H, V>(headers: &mut HeaderMap, header: H, value: V) -> Result
where
    H: IntoHeaderName,
    V: TryInto<HeaderValue>,
    Error: From<V::Error>,
{
    let value = value.try_into()?;
    headers.insert(header, value);
    Ok(())
}

fn header_insert_if_missing<H, V>(headers: &mut HeaderMap, header: H, value: V) -> Result
where
    H: IntoHeaderName,
    V: TryInto<HeaderValue>,
    Error: From<V::Error>,
{
    let value = value.try_into()?;
    headers.entry(header).or_insert(value);
    Ok(())
}

fn header_append<H, V>(headers: &mut HeaderMap, header: H, value: V) -> Result
where
    H: IntoHeaderName,
    V: TryInto<HeaderValue>,
    Error: From<V::Error>,
{
    let value = value.try_into()?;
    headers.append(header, value);
    Ok(())
}

/// Represents a request that's ready to be sent. You can inspect this object for information about the request.
#[derive(Debug)]
pub struct PreparedRequest<B> {
    url: Url,
    method: Method,
    body: B,
    pub(crate) base_settings: BaseSettings,
}

#[cfg(test)]
impl PreparedRequest<Vec<u8>> {
    pub(crate) fn new<U>(method: Method, base_url: U) -> Self
    where
        U: AsRef<str>,
    {
        PreparedRequest {
            url: Url::parse(base_url.as_ref()).unwrap(),
            method,
            body: Vec::new(),
            base_settings: BaseSettings::default(),
        }
    }
}

impl<B> PreparedRequest<B> {
    #[cfg(not(feature = "compress"))]
    fn set_compression(&mut self) -> Result {
        Ok(())
    }

    #[cfg(feature = "compress")]
    fn set_compression(&mut self) -> Result {
        if self.base_settings.allow_compression {
            header_insert(&mut self.base_settings.headers, ACCEPT_ENCODING, "gzip, deflate")?;
        }
        Ok(())
    }

    fn base_redirect_url(&self, location: &str, previous_url: &Url) -> Result<Url> {
        match Url::parse(location) {
            Ok(url) => Ok(url),
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                let joined_url = previous_url
                    .join(location)
                    .map_err(|_| InvalidResponseKind::RedirectionUrl)?;

                Ok(joined_url)
            }
            Err(_) => Err(InvalidResponseKind::RedirectionUrl.into()),
        }
    }

    fn write_headers<W>(&self, writer: &mut W) -> Result
    where
        W: Write,
    {
        for (key, value) in self.base_settings.headers.iter() {
            write!(writer, "{}: ", key.as_str())?;
            writer.write_all(value.as_bytes())?;
            write!(writer, "\r\n")?;
        }
        write!(writer, "\r\n")?;
        Ok(())
    }

    /// Get the URL of this request.
    pub fn url(&self) -> &Url {
        &self.url
    }

    /// Get the method of this request.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// Get the headers of this request.
    pub fn headers(&self) -> &HeaderMap {
        &self.base_settings.headers
    }
}

impl<B: AsRef<[u8]>> PreparedRequest<B> {
    /// Get the body of the request.
    ///
    /// If no body was provided, the slice will be empty.
    pub fn body(&self) -> &[u8] {
        self.body.as_ref()
    }

    fn has_body(&self) -> bool {
        !self.body.as_ref().is_empty() && self.method != Method::TRACE
    }

    fn write_request<W>(&self, writer: W, url: &Url) -> Result
    where
        W: Write,
    {
        let mut writer = BufWriter::new(writer);
        let version = Version::HTTP_11;

        if let Some(query) = url.query() {
            debug!("{} {}?{} {:?}", self.method.as_str(), url.path(), query, version);

            write!(
                writer,
                "{} {}?{} {:?}\r\n",
                self.method.as_str(),
                url.path(),
                query,
                version,
            )?;
        } else {
            debug!("{} {} {:?}", self.method.as_str(), url.path(), version);

            write!(writer, "{} {} {:?}\r\n", self.method.as_str(), url.path(), version)?;
        }

        self.write_headers(&mut writer)?;

        if self.has_body() {
            debug!("writing out body of length {}", self.body.as_ref().len());
            writer.write_all(self.body.as_ref())?;
        }

        writer.flush()?;

        Ok(())
    }

    /// Send this request and wait for the result.
    pub fn send(&mut self) -> Result<Response> {
        let mut url = Cow::Borrowed(&self.url);
        set_host(&mut self.base_settings.headers, &url)?;

        let mut redirections = 0;

        loop {
            let info = ConnectInfo {
                url: &url,
                base_settings: &self.base_settings,
            };
            let mut stream = BaseStream::connect(&info)?;
            self.write_request(&mut stream, &url)?;
            let resp = parse_response(stream, self)?;

            debug!("status code {}", resp.status().as_u16());

            let is_redirect = match resp.status() {
                StatusCode::MOVED_PERMANENTLY
                | StatusCode::FOUND
                | StatusCode::SEE_OTHER
                | StatusCode::TEMPORARY_REDIRECT
                | StatusCode::PERMANENT_REDIRECT => true,
                _ => false,
            };
            if !self.base_settings.follow_redirects || !is_redirect {
                return Ok(resp);
            }

            redirections += 1;
            if redirections > self.base_settings.max_redirections {
                return Err(ErrorKind::TooManyRedirections.into());
            }

            // Handle redirect
            let location = resp
                .headers()
                .get(http::header::LOCATION)
                .ok_or(InvalidResponseKind::LocationHeader)?;
            let location = location.to_str().map_err(|_| InvalidResponseKind::LocationHeader)?;

            url = Cow::Owned(self.base_redirect_url(location, &url)?);
            set_host(&mut self.base_settings.headers, &url)?;

            debug!("redirected to {} giving url {}", location, url);
        }
    }
}

fn set_host(headers: &mut HeaderMap, url: &Url) -> Result {
    let host = url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
    if let Some(port) = url.port() {
        header_insert(headers, HOST, format!("{}:{}", host, port))?;
    } else {
        header_insert(headers, HOST, host)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::{header_append, header_insert, header_insert_if_missing};
    use http::header::{HeaderMap, HeaderValue, USER_AGENT};

    #[test]
    fn test_header_insert_exists() {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("hello"));
        header_insert(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_insert_missing() {
        let mut headers = HeaderMap::new();
        header_insert(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_insert_if_missing_exists() {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("hello"));
        header_insert_if_missing(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "hello");
    }

    #[test]
    fn test_header_insert_if_missing_missing() {
        let mut headers = HeaderMap::new();
        header_insert_if_missing(&mut headers, USER_AGENT, "world").unwrap();
        assert_eq!(headers[USER_AGENT], "world");
    }

    #[test]
    fn test_header_append() {
        let mut headers = HeaderMap::new();
        header_append(&mut headers, USER_AGENT, "hello").unwrap();
        header_append(&mut headers, USER_AGENT, "world").unwrap();

        let vals: Vec<_> = headers.get_all(USER_AGENT).into_iter().collect();
        assert_eq!(vals.len(), 2);
        for val in vals {
            assert!(val == "hello" || val == "world");
        }
    }
}
