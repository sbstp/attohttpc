#![allow(dead_code)]
use std::convert::From;
use std::fmt::Display;
use std::io::{prelude::*, BufWriter};
use std::result;
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
use crate::error::{HttpError, Result};
use crate::parsing::{parse_response, ResponseReader};
use crate::streams::BaseStream;

pub trait HttpTryInto<T> {
    fn try_into(self) -> result::Result<T, http::Error>;
}

impl<T, U> HttpTryInto<U> for T
where
    U: HttpTryFrom<T>,
    http::Error: From<<U as http::HttpTryFrom<T>>::Error>,
{
    fn try_into(self) -> result::Result<U, http::Error> {
        let val = U::try_from(self)?;
        Ok(val)
    }
}

fn header_insert<H, V>(headers: &mut HeaderMap, header: H, value: V) -> Result
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.insert(header, value);
    Ok(())
}

fn header_append<H, V>(headers: &mut HeaderMap, header: H, value: V) -> Result
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.append(header, value);
    Ok(())
}

/// `Request` is the main way of performing requests.
///
/// You can create a `RequestBuilder` the hard way using the `new` or `try_new` method,
/// or use one of the simpler constructors available in the crate root, such as `get`
/// `post`, etc.
pub struct RequestBuilder {
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

impl RequestBuilder {
    /// Create a new `Request` with the base URL and the given method.
    ///
    /// # Panics
    /// Panics if the base url is invalid or if the method is CONNECT.
    pub fn new<U>(method: Method, base_url: U) -> RequestBuilder
    where
        U: AsRef<str>,
    {
        RequestBuilder::try_new(method, base_url).expect("invalid url or method")
    }

    /// Try to create a new `RequestBuilder`.
    ///
    /// If the base URL is invalid, an error is returned.
    /// If the method is CONNECT, an error is also returned. CONNECT is not yet supported.
    pub fn try_new<U>(method: Method, base_url: U) -> Result<RequestBuilder>
    where
        U: AsRef<str>,
    {
        let url = Url::parse(base_url.as_ref()).map_err(|_| HttpError::InvalidUrl("invalid base url"))?;

        match method {
            Method::CONNECT => return Err(HttpError::Other("CONNECT is not supported")),
            _ => {}
        }

        Ok(RequestBuilder {
            url,
            method: method,
            headers: HeaderMap::new(),
            body: Vec::new(),
            follow_redirects: true,
            #[cfg(feature = "charsets")]
            default_charset: None,
            #[cfg(feature = "compress")]
            allow_compression: true,
        })
    }

    /// Associate a query string parameter to the given value.
    ///
    /// The same key can be used multiple times.
    pub fn param<V>(mut self, key: &str, value: V) -> RequestBuilder
    where
        V: Display,
    {
        self.url.query_pairs_mut().append_pair(key, &format!("{}", value));
        self
    }

    /// Associated a list of pairs to query parameters.
    ///
    /// The same key can be used multiple times.
    pub fn params<'p, I, V>(mut self, pairs: I) -> RequestBuilder
    where
        I: Into<&'p [(&'p str, V)]>,
        V: Display + 'p,
    {
        for (key, value) in pairs.into() {
            self.url.query_pairs_mut().append_pair(key, &format!("{}", value));
        }
        self
    }

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    ///
    /// # Panics
    /// This method will panic if the value is invalid.
    pub fn header<H, V>(self, header: H, value: V) -> RequestBuilder
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        self.try_header(header, value).expect("invalid header value")
    }

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    ///
    /// # Panics
    /// This method will panic if the value is invalid.
    pub fn header_append<H, V>(self, header: H, value: V) -> RequestBuilder
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        self.try_header_append(header, value).expect("invalid header value")
    }

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    pub fn try_header<H, V>(mut self, header: H, value: V) -> Result<RequestBuilder>
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_insert(&mut self.headers, header, value)?;
        Ok(self)
    }

    /// Append a new header to this `Request`.
    ///
    /// The new header is always appended to the `Request`, even if the header already exists.
    pub fn try_header_append<H, V>(mut self, header: H, value: V) -> Result<RequestBuilder>
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_append(&mut self.headers, header, value)?;
        Ok(self)
    }

    /// Set the body of this request to be text.
    ///
    /// If the `Content-Type` header is unset, it will be set to `text/plain` and the carset to UTF-8.
    pub fn text(mut self, body: impl Into<String>) -> RequestBuilder {
        self.body = body.into().into_bytes();
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .unwrap()
            .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
        self
    }

    /// Set the body of this request to be bytes.
    ///
    /// The can be a `&[u8]` or a `str`, anything that's a sequence of bytes.
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn bytes(mut self, body: impl Into<Vec<u8>>) -> RequestBuilder {
        self.body = body.into();
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .unwrap()
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self
    }

    /// Set the body of this request to be the JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Result<RequestBuilder> {
        self.body = serde_json::to_vec(value)?;
        self.headers
            .entry(http::header::CONTENT_TYPE)
            .unwrap()
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        Ok(self)
    }

    /// Sets if this `Request` should follow redirects, 3xx codes.
    ///
    /// This value defaults to true.
    pub fn follow_redirects(mut self, follow_redirects: bool) -> RequestBuilder {
        self.follow_redirects = follow_redirects;
        self
    }

    /// Set the default charset to use while parsing the response of this `Request`.
    ///
    /// If the response does not say which charset it uses, this charset will be used to decode the request.
    /// This value defaults to `None`, in which case ISO-8859-1 is used.
    #[cfg(feature = "charsets")]
    pub fn default_charset(mut self, default_charset: Option<Charset>) -> RequestBuilder {
        self.default_charset = default_charset;
        self
    }

    /// Sets if this `Request` will announce that it accepts compression.
    ///
    /// This value defaults to true. Note that this only lets the browser know that this `Request` supports
    /// compression, the server might choose not to compress the content.
    #[cfg(feature = "compress")]
    pub fn allow_compression(mut self, allow_compression: bool) -> RequestBuilder {
        self.allow_compression = allow_compression;
        self
    }

    /// Create a `PreparedRequest` from this `RequestBuilder`.
    ///
    /// # Panics
    /// Will panic if an error occurs trying to prepare the request. It shouldn't happen.
    pub fn prepare(self) -> PreparedRequest {
        self.try_prepare().expect("failed to prepare request")
    }

    /// Create a `PreparedRequest` from this `RequestBuilder`.
    pub fn try_prepare(self) -> Result<PreparedRequest> {
        let mut prepped = PreparedRequest {
            url: self.url,
            method: self.method,
            headers: self.headers,
            body: self.body,
            follow_redirects: self.follow_redirects,
            #[cfg(feature = "charsets")]
            default_charset: self.default_charset,
            #[cfg(feature = "compress")]
            allow_compression: self.allow_compression,
        };

        header_insert(&mut prepped.headers, CONNECTION, "close")?;
        prepped.set_host(&prepped.url.clone())?;
        prepped.set_compression()?;
        if prepped.has_body() {
            header_insert(&mut prepped.headers, CONTENT_LENGTH, format!("{}", prepped.body.len()))?;
        }

        Ok(prepped)
    }

    /// Send this request directly.
    pub fn send(self) -> Result<(StatusCode, HeaderMap, ResponseReader)> {
        self.try_prepare()?.send()
    }
}

/// Represents a request that's ready to be sent. You can inspect this object for information about the request.
pub struct PreparedRequest {
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

impl PreparedRequest {
    #[cfg(test)]
    pub(crate) fn new<U>(method: Method, base_url: U) -> PreparedRequest
    where
        U: AsRef<str>,
    {
        PreparedRequest {
            url: Url::parse(base_url.as_ref()).unwrap(),
            method: method,
            headers: HeaderMap::new(),
            body: vec![],
            follow_redirects: true,
            #[cfg(feature = "charsets")]
            default_charset: None,
            #[cfg(feature = "compress")]
            allow_compression: true,
        }
    }

    fn set_host(&mut self, url: &Url) -> Result {
        let host = url.host_str().ok_or(HttpError::InvalidUrl("url has no host"))?;
        if let Some(port) = url.port() {
            header_insert(&mut self.headers, HOST, format!("{}:{}", host, port))?;
        } else {
            header_insert(&mut self.headers, HOST, host)?;
        }
        Ok(())
    }

    #[cfg(not(feature = "compress"))]
    fn set_compression(&mut self) -> Result {
        Ok(())
    }

    #[cfg(feature = "compress")]
    fn set_compression(&mut self) -> Result {
        if self.allow_compression {
            header_insert(&mut self.headers, ACCEPT_ENCODING, "gzip, deflate")?;
        }
        Ok(())
    }

    fn has_body(&self) -> bool {
        !self.body.is_empty() && self.method != Method::TRACE
    }

    fn base_redirect_url(&self, location: &str, previous_url: &Url) -> Result<Url> {
        Ok(match Url::parse(location) {
            Ok(url) => url,
            Err(url::ParseError::RelativeUrlWithoutBase) => previous_url
                .join(location)
                .map_err(|_| HttpError::InvalidUrl("cannot join location with new url"))?,
            Err(_) => Err(HttpError::InvalidUrl("invalid redirection url"))?,
        })
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

    fn write_request<W>(&mut self, writer: W, url: &Url) -> Result
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
            debug!("writing out body of length {}", self.body.len());
            writer.write_all(&self.body)?;
        }

        writer.flush()?;

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
        &self.headers
    }

    /// Get the body of the request.
    ///
    /// If no body was provided, the slice will be empty.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Send this request and wait for the result.
    pub fn send(mut self) -> Result<(StatusCode, HeaderMap, ResponseReader)> {
        let mut url = self.url.clone();
        loop {
            let mut stream = BaseStream::connect(&url)?;
            self.write_request(&mut stream, &url)?;
            let (status, headers, resp) = parse_response(stream, &self)?;

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

            url = self.base_redirect_url(location, &url)?;
            self.set_host(&url)?;

            debug!("redirected to {} giving url {}", location, url,);
        }
    }
}
