#[cfg(feature = "cookies")]
use std::sync::Arc;
#[cfg(feature = "cookies")]
use std::sync::RwLock;

#[cfg(feature = "cookies")]
use bytes::Bytes;
#[cfg(feature = "cookies")]
use cookie::Cookie as RawCookie;
#[cfg(feature = "cookies")]
use cookie_store::CookieStore;
#[cfg(feature = "cookies")]
use cookie_store::CookieStore;
use url::Url;

use crate::header::HeaderValue;

pub(crate) trait InternalJar: Clone + Default {
    fn new() -> Self;

    fn header_value_for_url(&self, url: &Url) -> Option<HeaderValue>;

    fn store_cookies_for_url<'a>(&self, url: &Url, set_cookie_headers: impl Iterator<Item = &'a HeaderValue>);
}

#[cfg(feature = "cookies")]
#[derive(Clone, Debug)]
pub struct CookieJarImpl {
    inner: Arc<RwLock<CookieStore>>,
}

#[derive(Clone, Debug)]
pub struct NoOpJar {}

#[cfg(feature = "cookies")]
pub type CookieJar = CookieJarImpl;

#[cfg(not(feature = "cookies"))]
pub type CookieJar = NoOpJar;

#[cfg(feature = "cookies")]
impl InternalJar for CookieJarImpl {
    fn new() -> Self {
        CookieJarImpl {
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
        let iter =
            set_cookie_headers.filter_map(|v| match RawCookie::parse(std::str::from_utf8(v.as_bytes()).unwrap()) {
                Ok(c) => Some(c.into_owned()),
                Err(err) => {
                    warn!("Invalid cookie could not be stored to jar: {}", err);
                    None
                }
            });
        self.inner.write().unwrap().store_response_cookies(iter, url)
    }
}

#[cfg(feature = "cookies")]
impl Default for CookieJarImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl InternalJar for NoOpJar {
    fn new() -> Self {
        NoOpJar {}
    }

    fn header_value_for_url(&self, _: &Url) -> Option<HeaderValue> {
        None
    }

    fn store_cookies_for_url<'a>(&self, _: &Url, _: impl Iterator<Item = &'a HeaderValue>) {}
}

impl Default for NoOpJar {
    fn default() -> Self {
        Self::new()
    }
}
