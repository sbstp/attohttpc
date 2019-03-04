use std::convert::From;
use std::fmt::Display;
use std::io::{prelude::*, BufWriter};
use std::str;

#[cfg(feature = "compress")]
use http::header::ACCEPT_ENCODING;
use http::{
    header::{HeaderValue, IntoHeaderName, CONNECTION, CONTENT_LENGTH, HOST},
    status::StatusCode,
    HeaderMap, HttpTryFrom, Method, Version,
};
use url::Url;

#[cfg(feature = "charsets")]
use crate::charsets::Charset;
use crate::error::{HttpError, HttpResult};
use crate::response::{self, ResponseReader};
use crate::streams::BaseStream;

pub trait HttpTryInto<T> {
    fn try_into(self) -> Result<T, http::Error>;
}

impl<T, U> HttpTryInto<U> for T
where
    U: HttpTryFrom<T>,
    http::Error: From<<U as http::HttpTryFrom<T>>::Error>,
{
    fn try_into(self) -> Result<U, http::Error> {
        let val = U::try_from(self)?;
        Ok(val)
    }
}

fn header_insert<H, V>(headers: &mut HeaderMap, header: H, value: V) -> HttpResult
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.insert(header, value);
    Ok(())
}

fn header_append<H, V>(headers: &mut HeaderMap, header: H, value: V) -> HttpResult
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.append(header, value);
    Ok(())
}

pub struct Request {
    url: Url,
    method: Method,
    headers: HeaderMap,
    body: Vec<u8>,
    follow_redirects: bool,
    #[cfg(feature = "charsets")]
    pub(crate) default_charset: Option<Charset>,
    #[cfg(feature = "compress")]
    allow_compression: bool,
}

impl Request {
    /// Create a new `Request` with the base URL and the given method.
    pub fn new(base_url: &str, method: Method) -> Request {
        let url = Url::parse(base_url).expect("invalid url");

        match method {
            Method::CONNECT => panic!("CONNECT is not yet supported"),
            _ => {}
        }

        Request {
            url,
            method: method,
            headers: HeaderMap::new(),
            body: Vec::new(),
            follow_redirects: true,
            #[cfg(feature = "charsets")]
            default_charset: None,
            #[cfg(feature = "compress")]
            allow_compression: true,
        }
    }

    /// Create a new `Request` with the GET method.
    pub fn get(base_url: &str) -> Request {
        Request::new(base_url, Method::GET)
    }

    /// Create a new `Request` with the POST method.
    pub fn post(base_url: &str) -> Request {
        Request::new(base_url, Method::POST)
    }

    /// Create a new `Request` with the PUT method.
    pub fn put(base_url: &str) -> Request {
        Request::new(base_url, Method::PUT)
    }

    /// Create a new `Request` with the DELETE method.
    pub fn delete(base_url: &str) -> Request {
        Request::new(base_url, Method::DELETE)
    }

    /// Create a new `Request` with the HEAD method.
    pub fn head(base_url: &str) -> Request {
        Request::new(base_url, Method::HEAD)
    }

    /// Create a new `Request` with the OPTIONS method.
    pub fn options(base_url: &str) -> Request {
        Request::new(base_url, Method::OPTIONS)
    }

    /// Create a new `Request` with the PATCH method.
    pub fn patch(base_url: &str) -> Request {
        Request::new(base_url, Method::PATCH)
    }

    /// Create a new `Request` with the TRACE method.
    pub fn trace(base_url: &str) -> Request {
        Request::new(base_url, Method::TRACE)
    }

    /// Associate a query string parameter to the given value.
    ///
    /// The same key can be used multiple times.
    pub fn param<V>(&mut self, key: &str, value: V)
    where
        V: Display,
    {
        self.url.query_pairs_mut().append_pair(key, &format!("{}", value));
    }

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    pub fn header<H, V>(&mut self, header: H, value: V) -> HttpResult
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_insert(&mut self.headers, header, value)
    }

    /// Append a new header to this `Request`.
    ///
    /// The new header is always appended to the `Request`, even if the header already exists.
    pub fn header_append<H, V>(&mut self, header: H, value: V) -> HttpResult
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_append(&mut self.headers, header, value)
    }

    pub fn body(&mut self, body: impl AsRef<[u8]>) {
        self.body = body.as_ref().to_owned();
    }

    /// Sets if this `Request` should follow redirects, 3xx codes.
    ///
    /// This value defaults to true.
    pub fn follow_redirects(&mut self, follow_redirects: bool) {
        self.follow_redirects = follow_redirects;
    }

    /// Set the default charset to use while parsing the response of this `Request`.
    ///
    /// If the response does not say which charset it uses, this charset will be used to decode the request.
    /// This value defaults to `None`, in which case ISO-8859-1 is used.
    #[cfg(feature = "charsets")]
    pub fn default_charset(&mut self, default_charset: Option<Charset>) {
        self.default_charset = default_charset;
    }

    /// Sets if this `Request` will announce that it accepts compression.
    ///
    /// This value defaults to true. Note that this only lets the browser know that this `Request` supports
    /// compression, the server might choose not to compress the content.
    #[cfg(feature = "compress")]
    pub fn allow_compression(&mut self, allow_compression: bool) {
        self.allow_compression = allow_compression;
    }

    fn base_redirect_url(&self, location: &str, previous_url: &Url) -> HttpResult<Url> {
        Ok(match Url::parse(location) {
            Ok(url) => url,
            Err(url::ParseError::RelativeUrlWithoutBase) => previous_url
                .join(location)
                .map_err(|_| HttpError::InvalidUrl("cannot join location with new url"))?,
            Err(_) => Err(HttpError::InvalidUrl("invalid redirection url"))?,
        })
    }

    /// Send this `Request` to the server.
    ///
    /// This method consumes the object so that it cannot be used after sending the request.
    pub fn send(mut self) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
        let mut url = self.url.clone();
        loop {
            let mut stream = BaseStream::connect(&url)?;
            self.write_request(&mut stream, &url)?;
            let (status, headers, resp) = response::read_response(stream, &self)?;

            debug!("status code {}", status.as_u16());

            if !self.follow_redirects || !status.is_redirection() {
                return Ok((status, headers, resp));
            }

            // Handle redirect
            let location = headers
                .get(http::header::LOCATION)
                .ok_or(HttpError::InvalidResponse("redirect has no location header"))?;
            let location = location
                .to_str()
                .map_err(|_| HttpError::InvalidResponse("location to str error"))?;

            let new_url = self.base_redirect_url(location, &url)?;
            url = new_url;

            debug!("redirected to {} giving url {}", location, url,);
        }
    }

    fn write_request<W>(&mut self, writer: W, url: &Url) -> HttpResult
    where
        W: Write,
    {
        let mut writer = BufWriter::new(writer);
        let version = Version::HTTP_11;
        let has_body = !self.body.is_empty() && self.method != Method::TRACE;

        if let Some(query) = url.query() {
            debug!("{} {}?{} {:?}", self.method.as_str(), url.path(), query, version,);

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

            write!(writer, "{} {} {:?}\r\n", self.method.as_str(), url.path(), version,)?;
        }

        header_insert(&mut self.headers, CONNECTION, "close")?;

        let host = url.host_str().ok_or(HttpError::InvalidUrl("url has no host"))?;
        if let Some(port) = url.port() {
            header_insert(&mut self.headers, HOST, format!("{}:{}", host, port))?;
        } else {
            header_insert(&mut self.headers, HOST, host)?;
        }

        if has_body {
            header_insert(&mut self.headers, CONTENT_LENGTH, format!("{}", self.body.len()))?;
        }

        self.compression_header()?;

        self.write_headers(&mut writer)?;

        if has_body {
            debug!("writing out body of length {}", self.body.len());
            writer.write_all(&self.body)?;
        }

        writer.flush()?;

        Ok(())
    }

    fn write_headers<W>(&self, writer: &mut W) -> HttpResult
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

    #[cfg(feature = "compress")]
    fn compression_header(&mut self) -> HttpResult {
        if self.allow_compression {
            header_insert(&mut self.headers, ACCEPT_ENCODING, "gzip, deflate")?;
        }
        Ok(())
    }

    #[cfg(not(feature = "compress"))]
    fn compression_header(&mut self) -> HttpResult {
        Ok(())
    }
}
