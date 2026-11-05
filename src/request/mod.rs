use std::convert::{From, TryInto};
use std::io::{prelude::*, BufWriter};
use std::str;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "flate2")]
use http::header::ACCEPT_ENCODING;
use http::{
    header::{
        HeaderValue, IntoHeaderName, AUTHORIZATION, CONTENT_LENGTH, CONTENT_TYPE, COOKIE, HOST, PROXY_AUTHORIZATION,
        TRANSFER_ENCODING, WWW_AUTHENTICATE,
    },
    HeaderMap, Method, StatusCode, Version,
};
use url::Url;

use crate::error::{Error, ErrorKind, InvalidResponseKind, Result};
use crate::parsing::{parse_response, Response};
use crate::streams::{BaseStream, ConnectInfo};

/// Contains types to describe request bodies
pub mod body;
mod builder;
pub mod proxy;
mod session;
mod settings;

use body::{Body, BodyKind};
pub use builder::{RequestBuilder, RequestInspector};
pub use session::Session;
pub(crate) use settings::BaseSettings;

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
    headers: HeaderMap,
    pub(crate) base_settings: Arc<BaseSettings>,
    /// Set when a redirect changed the method to GET and the request body must be dropped
    /// (RFC 9110, section 15.4).
    skip_body: bool,
}

#[cfg(test)]
impl PreparedRequest<body::Empty> {
    pub(crate) fn new<U>(method: Method, base_url: U) -> Self
    where
        U: AsRef<str>,
    {
        PreparedRequest {
            url: Url::parse(base_url.as_ref()).unwrap(),
            method,
            body: body::Empty,
            headers: HeaderMap::new(),
            base_settings: Arc::new(BaseSettings::default()),
            skip_body: false,
        }
    }
}

impl<B> PreparedRequest<B> {
    #[cfg(not(feature = "flate2"))]
    fn set_compression(&mut self) -> Result {
        Ok(())
    }

    #[cfg(feature = "flate2")]
    fn set_compression(&mut self) -> Result {
        if self.base_settings.allow_compression {
            header_insert(&mut self.headers, ACCEPT_ENCODING, "gzip, deflate")?;
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
        for (key, value) in self.headers.iter() {
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

    /// Get the body of the request.
    pub fn body(&self) -> &B {
        &self.body
    }

    /// Get the headers of this request.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }
}

impl<B: Body> PreparedRequest<B> {
    fn write_request<W>(&mut self, writer: W, url: &Url, proxy: Option<&Url>) -> Result
    where
        W: Write,
    {
        let mut writer = BufWriter::new(writer);
        let version = Version::HTTP_11;

        if proxy.is_some() && url.scheme() == "http" {
            debug!("{} {} {:?}", self.method.as_str(), url, version);

            write!(writer, "{} {} {:?}\r\n", self.method.as_str(), url, version)?;

            if let Some(auth) = proxy.and_then(proxy::proxy_authorization_value) {
                write!(writer, "proxy-authorization: {auth}\r\n")?;
            }
        } else if let Some(query) = url.query() {
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

        if !self.skip_body {
            match self.body.kind()? {
                BodyKind::Empty => (),
                BodyKind::KnownLength(len) => {
                    debug!("writing out body of length {}", len);
                    self.body.write(&mut writer)?;
                }
                BodyKind::Chunked => {
                    debug!("writing out chunked body");
                    let mut writer = body::ChunkedWriter(&mut writer);
                    self.body.write(&mut writer)?;
                    writer.close()?;
                }
            }
        }

        writer.flush()?;

        Ok(())
    }

    /// Send this request and wait for the result.
    pub fn send(&mut self) -> Result<Response> {
        let mut url = self.url.clone();

        let deadline = self.base_settings.timeout.map(|timeout| Instant::now() + timeout);
        let mut redirections = 0;

        loop {
            // If a proxy is set and the url is using http, we must connect to the proxy and send
            // a request with an authority instead of a path.
            //
            // If a proxy is set and the url is using https, we must connect to the proxy using
            // the CONNECT method, and then send https traffic on the socket after the CONNECT
            // handshake.

            let proxy = self.base_settings.proxy_settings.for_url(&url).cloned();

            // RFC 9112 section 3.2: the Host header must be the authority of the target
            // URI, even when the request is sent through a proxy in absolute-form.
            set_host(&mut self.headers, &url)?;

            let info = ConnectInfo {
                url: &url,
                proxy: proxy.as_ref(),
                base_settings: &self.base_settings,
                deadline,
            };
            let mut stream = BaseStream::connect(&info)?;

            self.write_request(&mut stream, &url, proxy.as_ref())?;
            let resp = parse_response(stream, self, &url)?;

            debug!("status code {}", resp.status().as_u16());

            let is_redirect = matches!(
                resp.status(),
                StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
                    | StatusCode::SEE_OTHER
                    | StatusCode::TEMPORARY_REDIRECT
                    | StatusCode::PERMANENT_REDIRECT
            );
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

            let location = String::from_utf8_lossy(location.as_bytes());

            // 301 and 302 responses may change the request method from POST to GET, and 303
            // responses are followed with a GET unless the method is HEAD (RFC 9110, section
            // 15.4). 307 and 308 responses preserve the method and body.
            match resp.status() {
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND if self.method == Method::POST => {
                    self.change_method_to_get();
                }
                StatusCode::SEE_OTHER if self.method != Method::HEAD => {
                    self.change_method_to_get();
                }
                _ => (),
            }

            let previous_url = url.clone();
            url = self.base_redirect_url(&location, &url)?;

            // Credentials are scoped to the origin they were issued for (RFC 9110, section 11.5),
            // so they must not be forwarded to a redirect target with a different scheme, host or
            // port (RFC 9110, section 15.4).
            if is_cross_origin_redirect(&url, &previous_url) {
                debug!("redirect crossed origins, removing sensitive headers");
                self.headers.remove(AUTHORIZATION);
                self.headers.remove(COOKIE);
                self.headers.remove("cookie2");
                self.headers.remove(PROXY_AUTHORIZATION);
                self.headers.remove(WWW_AUTHENTICATE);
            }

            debug!("redirected to {} giving url {}", location, url);
        }
    }
    /// Change the method to GET and drop the body and content-specific headers, as required
    /// when following certain redirects (RFC 9110, section 15.4).
    fn change_method_to_get(&mut self) {
        debug!("changing method {} to GET for redirect", self.method);
        self.method = Method::GET;
        self.skip_body = true;
        self.headers.remove(CONTENT_LENGTH);
        self.headers.remove(CONTENT_TYPE);
        self.headers.remove(TRANSFER_ENCODING);
    }
}

/// Check whether a redirect sends the request to a different origin.
///
/// Two URLs are same-origin when they have the same scheme, host and effective port
/// (using the scheme's default port when none is specified).
fn is_cross_origin_redirect(next: &Url, previous: &Url) -> bool {
    next.scheme() != previous.scheme()
        || next.host_str() != previous.host_str()
        || next.port_or_known_default() != previous.port_or_known_default()
}

fn set_host(headers: &mut HeaderMap, url: &Url) -> Result {
    let host = url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
    if let Some(port) = url.port() {
        header_insert(headers, HOST, format!("{host}:{port}"))?;
    } else {
        header_insert(headers, HOST, host)?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use std::sync::Arc;

    use http::header::{HeaderMap, HeaderValue, USER_AGENT};
    use http::Method;
    use url::Url;

    use super::BaseSettings;
    use super::{
        header_append, header_insert, header_insert_if_missing, is_cross_origin_redirect, set_host, PreparedRequest,
    };
    use crate::body::Empty;

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

    #[test]
    fn test_is_cross_origin_redirect() {
        let base = Url::parse("https://example.com/path").unwrap();

        // Same origin
        assert!(!is_cross_origin_redirect(
            &Url::parse("https://example.com/other").unwrap(),
            &base
        ));
        assert!(!is_cross_origin_redirect(
            &Url::parse("https://example.com:443/other").unwrap(),
            &base
        ));

        // Different host
        assert!(is_cross_origin_redirect(
            &Url::parse("https://other.com/path").unwrap(),
            &base
        ));

        // Different port
        assert!(is_cross_origin_redirect(
            &Url::parse("https://example.com:8443/path").unwrap(),
            &base
        ));

        // Different scheme (https -> http downgrade)
        assert!(is_cross_origin_redirect(
            &Url::parse("http://example.com/path").unwrap(),
            &base
        ));
    }

    #[test]
    fn test_http_url_with_http_proxy() {
        let mut req = PreparedRequest {
            method: Method::GET,
            url: Url::parse("http://reddit.com/r/rust").unwrap(),
            body: Empty,
            headers: HeaderMap::new(),
            base_settings: Arc::new(BaseSettings::default()),
            skip_body: false,
        };

        // mimic what send() does before calling write_request
        set_host(&mut req.headers, &req.url).unwrap();

        let proxy = Url::parse("http://proxy:3128").unwrap();
        let mut buf: Vec<u8> = vec![];
        req.write_request(&mut buf, &req.url.clone(), Some(&proxy)).unwrap();

        let text = std::str::from_utf8(&buf).unwrap();
        let lines: Vec<_> = text.split("\r\n").collect();

        assert_eq!(lines[0], "GET http://reddit.com/r/rust HTTP/1.1");
        // the Host header must be the authority of the target URI, not the proxy
        assert!(lines.contains(&"host: reddit.com"));
        assert!(!text.contains("proxy-authorization"));
    }

    #[test]
    fn test_http_url_with_https_proxy() {
        let mut req = PreparedRequest {
            method: Method::GET,
            url: Url::parse("http://reddit.com/r/rust").unwrap(),
            body: Empty,
            headers: HeaderMap::new(),
            base_settings: Arc::new(BaseSettings::default()),
            skip_body: false,
        };

        let proxy = Url::parse("http://proxy:3128").unwrap();
        let mut buf: Vec<u8> = vec![];
        req.write_request(&mut buf, &req.url.clone(), Some(&proxy)).unwrap();

        let text = std::str::from_utf8(&buf).unwrap();
        let lines: Vec<_> = text.split("\r\n").collect();

        assert_eq!(lines[0], "GET http://reddit.com/r/rust HTTP/1.1");
    }

    #[test]
    fn test_http_url_with_http_proxy_auth() {
        let mut req = PreparedRequest {
            method: Method::GET,
            url: Url::parse("http://reddit.com/r/rust").unwrap(),
            body: Empty,
            headers: HeaderMap::new(),
            base_settings: Arc::new(BaseSettings::default()),
            skip_body: false,
        };

        let proxy = Url::parse("http://user:p%40ss@proxy:3128").unwrap();
        let mut buf: Vec<u8> = vec![];
        req.write_request(&mut buf, &req.url.clone(), Some(&proxy)).unwrap();

        let text = std::str::from_utf8(&buf).unwrap();

        // credentials are percent-decoded before being encoded with Basic auth
        assert!(text.contains("proxy-authorization: Basic dXNlcjpwQHNz\r\n"));
    }

    #[test]
    fn test_http_url_with_http_proxy_auth_empty_credentials() {
        let mut req = PreparedRequest {
            method: Method::GET,
            url: Url::parse("http://reddit.com/r/rust").unwrap(),
            body: Empty,
            headers: HeaderMap::new(),
            base_settings: Arc::new(BaseSettings::default()),
            skip_body: false,
        };

        let proxy = Url::parse("http://proxy:3128").unwrap();
        let mut buf: Vec<u8> = vec![];
        req.write_request(&mut buf, &req.url.clone(), Some(&proxy)).unwrap();

        let text = std::str::from_utf8(&buf).unwrap();

        // no Proxy-Authorization header is sent when the proxy has no credentials
        assert!(!text.contains("proxy-authorization"));
    }
}
