use std::borrow::Borrow;
use std::convert::{From, TryInto};
use std::str;
#[cfg(feature = "tls-rustls")]
use std::sync::Arc;
use std::time::Duration;

use http::{
    header::{HeaderValue, IntoHeaderName, ACCEPT, CONNECTION, CONTENT_LENGTH, USER_AGENT},
    Method,
};
#[cfg(feature = "tls-rustls")]
use rustls::ClientConfig;
use url::Url;

#[cfg(feature = "charsets")]
use crate::charsets::Charset;
use crate::error::{Error, ErrorKind, Result};
use crate::parsing::Response;
use crate::request::{header_append, header_insert, header_insert_if_missing, BaseSettings, PreparedRequest};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// `RequestBuilder` is the main way of building requests.
///
/// You can create a `RequestBuilder` using the `new` or `try_new` method, but the recommended way
/// or use one of the simpler constructors available in the crate root or on the `Session` struct,
/// such as `get`, `post`, etc.
#[derive(Debug)]
pub struct RequestBuilder<B = [u8; 0]> {
    url: Url,
    method: Method,
    body: B,
    base_settings: BaseSettings,
}

impl RequestBuilder {
    /// Create a new `RequestBuilder` with the base URL and the given method.
    ///
    /// # Panics
    /// Panics if the base url is invalid or if the method is CONNECT.
    pub fn new<U>(method: Method, base_url: U) -> Self
    where
        U: AsRef<str>,
    {
        Self::try_new(method, base_url).expect("invalid url or method")
    }

    /// Try to create a new `RequestBuilder`.
    ///
    /// If the base URL is invalid, an error is returned.
    /// If the method is CONNECT, an error is also returned. CONNECT is not yet supported.
    pub fn try_new<U>(method: Method, base_url: U) -> Result<Self>
    where
        U: AsRef<str>,
    {
        Self::try_with_settings(method, base_url, BaseSettings::default())
    }

    pub(crate) fn with_settings<U>(method: Method, base_url: U, base_settings: BaseSettings) -> Self
    where
        U: AsRef<str>,
    {
        Self::try_with_settings(method, base_url, base_settings).expect("invalid url or method")
    }

    pub(crate) fn try_with_settings<U>(method: Method, base_url: U, base_settings: BaseSettings) -> Result<Self>
    where
        U: AsRef<str>,
    {
        let url = Url::parse(base_url.as_ref()).map_err(|_| ErrorKind::InvalidBaseUrl)?;

        if method == Method::CONNECT {
            return Err(ErrorKind::ConnectNotSupported.into());
        }

        Ok(Self {
            url,
            method,
            body: [],
            base_settings,
        })
    }
}

impl<B> RequestBuilder<B> {
    /// Associate a query string parameter to the given value.
    ///
    /// The same key can be used multiple times.
    pub fn param<K, V>(mut self, key: K, value: V) -> Self
    where
        K: AsRef<str>,
        V: ToString,
    {
        self.url.query_pairs_mut().append_pair(key.as_ref(), &value.to_string());
        self
    }

    /// Associated a list of pairs to query parameters.
    ///
    /// The same key can be used multiple times.
    ///
    /// # Example
    /// ```
    /// attohttpc::get("http://foo.bar").params(&[("p1", "v1"), ("p2", "v2")]);
    /// ```
    pub fn params<P, K, V>(mut self, pairs: P) -> Self
    where
        P: IntoIterator,
        P::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: ToString,
    {
        for pair in pairs.into_iter() {
            let (key, value) = pair.borrow();
            self.url.query_pairs_mut().append_pair(key.as_ref(), &value.to_string());
        }
        self
    }

    /// Enable HTTP basic authentication.
    ///
    /// This is available only on Linux and when TLS support is enabled.
    #[cfg(all(
        feature = "tls",
        not(any(target_os = "windows", target_os = "macos", target_os = "ios"))
    ))]
    pub fn basic_auth(self, username: impl std::fmt::Display, password: Option<impl std::fmt::Display>) -> Self {
        let auth = match password {
            Some(password) => format!("{}:{}", username, password),
            None => format!("{}:", username),
        };
        self.header(
            http::header::AUTHORIZATION,
            format!("Basic {}", openssl::base64::encode_block(auth.as_bytes())),
        )
    }

    /// Enable HTTP bearer authentication.
    pub fn bearer_auth(self, token: impl Into<String>) -> Self {
        self.header(http::header::AUTHORIZATION, format!("Bearer {}", token.into()))
    }

    fn body(self, body: impl AsRef<[u8]>) -> RequestBuilder<impl AsRef<[u8]>> {
        RequestBuilder {
            url: self.url,
            method: self.method,
            body,
            base_settings: self.base_settings,
        }
    }

    /// Set the body of this request to be text.
    ///
    /// If the `Content-Type` header is unset, it will be set to `text/plain` and the carset to UTF-8.
    pub fn text(mut self, body: impl AsRef<str>) -> RequestBuilder<impl AsRef<[u8]>> {
        struct Text<B1>(B1);

        impl<B1: AsRef<str>> AsRef<[u8]> for Text<B1> {
            fn as_ref(&self) -> &[u8] {
                self.0.as_ref().as_bytes()
            }
        }

        self.base_settings
            .headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("text/plain; charset=utf-8"));
        self.body(Text(body))
    }

    /// Set the body of this request to be bytes.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/octet-stream`.
    pub fn bytes(mut self, body: impl AsRef<[u8]>) -> RequestBuilder<impl AsRef<[u8]>> {
        self.base_settings
            .headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/octet-stream"));
        self.body(body)
    }

    /// Set the body of this request to be the JSON representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/json` and the charset to UTF-8.
    #[cfg(feature = "json")]
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Result<RequestBuilder<impl AsRef<[u8]>>> {
        let body = serde_json::to_vec(value)?;
        self.base_settings
            .headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json; charset=utf-8"));
        Ok(self.body(body))
    }

    /// Set the body of this request to be the URL-encoded representation of the given object.
    ///
    /// If the `Content-Type` header is unset, it will be set to `application/x-www-form-urlencoded`.
    #[cfg(feature = "form")]
    pub fn form<T: serde::Serialize>(mut self, value: &T) -> Result<RequestBuilder<impl AsRef<[u8]>>> {
        let body = serde_urlencoded::to_string(value)?.into_bytes();
        self.base_settings
            .headers
            .entry(http::header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/x-www-form-urlencoded"));
        Ok(self.body(body))
    }

    //
    // Settings
    //

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    ///
    /// # Panics
    /// This method will panic if the value is invalid.
    pub fn header<H, V>(self, header: H, value: V) -> Self
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
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
    pub fn header_append<H, V>(self, header: H, value: V) -> Self
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        self.try_header_append(header, value).expect("invalid header value")
    }

    /// Modify a header for this `Request`.
    ///
    /// If the header is already present, the value will be replaced. If you wish to append a new header,
    /// use `header_append`.
    pub fn try_header<H, V>(mut self, header: H, value: V) -> Result<Self>
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        header_insert(&mut self.base_settings.headers, header, value)?;
        Ok(self)
    }

    /// Append a new header to this `Request`.
    ///
    /// The new header is always appended to the `Request`, even if the header already exists.
    pub fn try_header_append<H, V>(mut self, header: H, value: V) -> Result<Self>
    where
        H: IntoHeaderName,
        V: TryInto<HeaderValue>,
        Error: From<V::Error>,
    {
        header_append(&mut self.base_settings.headers, header, value)?;
        Ok(self)
    }

    /// Set the maximum number of redirections this `Request` can perform.
    pub fn max_redirections(mut self, max_redirections: u32) -> Self {
        self.base_settings.max_redirections = max_redirections;
        self
    }

    /// Sets if this `Request` should follow redirects, 3xx codes.
    ///
    /// This value defaults to true.
    pub fn follow_redirects(mut self, follow_redirects: bool) -> Self {
        self.base_settings.follow_redirects = follow_redirects;
        self
    }

    /// Sets a connect timeout for this request.
    ///
    /// The default is 30 seconds.
    pub fn connect_timeout(mut self, duration: Duration) -> Self {
        self.base_settings.connect_timeout = duration;
        self
    }

    /// Sets a read timeout for this request.
    ///
    /// The default is 30 seconds.
    pub fn read_timeout(mut self, duration: Duration) -> Self {
        self.base_settings.read_timeout = duration;
        self
    }

    /// Sets a timeout for the whole request.
    ///
    /// Applies after a TCP connection is established. Defaults to no timeout.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.base_settings.timeout = Some(duration);
        self
    }

    /// Set the default charset to use while parsing the response of this `Request`.
    ///
    /// If the response does not say which charset it uses, this charset will be used to decode the request.
    /// This value defaults to `None`, in which case ISO-8859-1 is used.
    #[cfg(feature = "charsets")]
    pub fn default_charset(mut self, default_charset: Option<Charset>) -> Self {
        self.base_settings.default_charset = default_charset;
        self
    }

    /// Sets if this `Request` will announce that it accepts compression.
    ///
    /// This value defaults to true. Note that this only lets the browser know that this `Request` supports
    /// compression, the server might choose not to compress the content.
    #[cfg(feature = "compress")]
    pub fn allow_compression(mut self, allow_compression: bool) -> Self {
        self.base_settings.allow_compression = allow_compression;
        self
    }

    /// Sets if this `Request` will accept invalid TLS certificates.
    ///
    /// Accepting invalid certificates implies that invalid hostnames are accepted
    /// as well.
    ///
    /// The default value is `false`.
    ///
    /// # Danger
    /// Use this setting with care. This will accept **any** TLS certificate valid or not.
    /// If you are using self signed certificates, it is much safer to add their root CA
    /// to the list of trusted root CAs by your system.
    #[cfg(feature = "tls")]
    pub fn danger_accept_invalid_certs(mut self, accept_invalid_certs: bool) -> Self {
        self.base_settings.accept_invalid_certs = accept_invalid_certs;
        self
    }

    /// Sets if this `Request` will accept an invalid hostname in a TLS certificate.
    ///
    /// The default value is `false`.
    ///
    /// # Danger
    /// Use this setting with care. This will accept TLS certificates that do not match
    /// the hostname.
    #[cfg(feature = "tls")]
    pub fn danger_accept_invalid_hostnames(mut self, accept_invalid_hostnames: bool) -> Self {
        self.base_settings.accept_invalid_hostnames = accept_invalid_hostnames;
        self
    }

    /// Sets the TLS client configuration
    ///
    /// Defaults to a configuration using the root certificates
    /// from the webpki-roots crate.
    #[cfg(feature = "tls-rustls")]
    pub fn client_config(mut self, client_config: impl Into<Arc<ClientConfig>>) -> Self {
        self.base_settings.client_config = Some(client_config.into()).into();
        self
    }
}

impl<B: AsRef<[u8]>> RequestBuilder<B> {
    /// Create a `PreparedRequest` from this `RequestBuilder`.
    ///
    /// # Panics
    /// Will panic if an error occurs trying to prepare the request. It shouldn't happen.
    pub fn prepare(self) -> PreparedRequest<B> {
        self.try_prepare().expect("failed to prepare request")
    }

    /// Create a `PreparedRequest` from this `RequestBuilder`.
    pub fn try_prepare(self) -> Result<PreparedRequest<B>> {
        let mut prepped = PreparedRequest {
            url: self.url,
            method: self.method,
            body: self.body,
            base_settings: self.base_settings,
        };

        header_insert(&mut prepped.base_settings.headers, CONNECTION, "close")?;
        prepped.set_compression()?;
        if prepped.has_body() {
            header_insert(
                &mut prepped.base_settings.headers,
                CONTENT_LENGTH,
                prepped.body.as_ref().len(),
            )?;
        }

        header_insert_if_missing(&mut prepped.base_settings.headers, ACCEPT, "*/*")?;
        header_insert_if_missing(
            &mut prepped.base_settings.headers,
            USER_AGENT,
            format!("attohttpc/{}", VERSION),
        )?;

        Ok(prepped)
    }

    /// Send this request directly.
    pub fn send(self) -> Result<Response> {
        self.try_prepare()?.send()
    }
}

#[test]
#[cfg(feature = "tls")]
fn test_accept_invalid_certs_disabled_by_default() {
    let builder = RequestBuilder::new(Method::GET, "https://localhost:7900");
    assert_eq!(builder.base_settings.accept_invalid_certs, false);
    assert_eq!(builder.base_settings.accept_invalid_hostnames, false);

    let prepped = builder.prepare();
    assert_eq!(prepped.base_settings.accept_invalid_certs, false);
    assert_eq!(prepped.base_settings.accept_invalid_hostnames, false);
}
