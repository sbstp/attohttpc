use url::Url;

use crate::header::HeaderValue;

pub(crate) trait InternalJar: Clone + Default {
    fn new() -> Self;

    fn header_value_for_url(&self, url: &Url) -> Option<HeaderValue>;

    fn store_cookies_for_url<'a>(&self, url: &Url, set_cookie_headers: impl Iterator<Item = &'a HeaderValue>);
}

#[cfg(feature = "cookies")]
mod jar {
    use std::sync::Arc;
    use std::sync::RwLock;

    use bytes::Bytes;
    use cookie::Cookie as RawCookie;
    use cookie_store::CookieStore;
    use url::Url;

    use super::InternalJar;
    use crate::header::HeaderValue;

    /// Persists cookies between requests.
    ///
    /// All the typical cookie properties, such as expiry, secure and http-only are respected.
    #[derive(Clone, Debug)]
    pub struct CookieJar {
        inner: Arc<RwLock<CookieStore>>,
    }

    impl InternalJar for CookieJar {
        fn new() -> Self {
            CookieJar {
                inner: Arc::new(RwLock::new(CookieStore::default())),
            }
        }

        fn header_value_for_url(&self, url: &Url) -> Option<HeaderValue> {
            // Credit: This code is basically taken from reqwest's CookieJar as-is.
            // https://docs.rs/reqwest/latest/src/reqwest/cookie.rs.html

            let hvalue = self
                .inner
                .read()
                .unwrap()
                .get_request_values(url)
                .map(|(name, value)| format!("{}={}", name, value))
                .collect::<Vec<_>>()
                .join("; ");

            if hvalue.is_empty() {
                return None;
            }

            HeaderValue::from_maybe_shared(Bytes::from(hvalue)).ok()
        }

        fn store_cookies_for_url<'a>(&self, url: &Url, set_cookie_headers: impl Iterator<Item = &'a HeaderValue>) {
            let iter = set_cookie_headers.filter_map(|v| match parse_cookie(v.as_bytes()) {
                Ok(c) => Some(c.into_owned()),
                Err(err) => {
                    warn!("Invalid cookie could not be stored to jar: {}", err);
                    None
                }
            });
            self.inner.write().unwrap().store_response_cookies(iter, url)
        }
    }

    impl CookieJar {
        /// Remove all the cookies stored in the [CookieJar].
        pub fn clear(&self) {
            self.inner.write().unwrap().clear();
        }
    }

    fn parse_cookie(buf: &[u8]) -> Result<RawCookie, Box<dyn std::error::Error>> {
        let s = std::str::from_utf8(buf)?;
        let c = RawCookie::parse(s)?;
        Ok(c)
    }

    impl Default for CookieJar {
        fn default() -> Self {
            Self::new()
        }
    }
}

mod dummy {
    use url::Url;

    use super::InternalJar;
    use crate::header::HeaderValue;

    #[derive(Clone, Debug)]
    pub struct DummyJar {}

    impl super::InternalJar for DummyJar {
        fn new() -> Self {
            DummyJar {}
        }

        fn header_value_for_url(&self, _: &Url) -> Option<HeaderValue> {
            None
        }

        fn store_cookies_for_url<'a>(&self, _: &Url, _: impl Iterator<Item = &'a HeaderValue>) {}
    }

    impl Default for DummyJar {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "cookies")]
pub use jar::CookieJar;

#[cfg(not(feature = "cookies"))]
pub use dummy::DummyJar as CookieJar;
