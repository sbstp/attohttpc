use std::{cell::RefCell, rc::Rc};

use bytes::Bytes;
use cookie::Cookie as RawCookie;
use cookie_store::CookieStore;
use url::Url;

use crate::header::HeaderValue;

/// Persists cookies between requests.
///
/// All the typical cookie properties, such as expiry, secure and http-only are respected.
#[derive(Clone, Debug)]
pub struct CookieJar(Rc<RefCell<CookieStore>>);

impl CookieJar {
    pub(crate) fn new() -> Self {
        CookieJar(Rc::new(RefCell::new(CookieStore::default())))
    }

    pub(crate) fn header_value_for_url(&self, url: &Url) -> Option<HeaderValue> {
        let hvalue = self
            .0
            .borrow()
            .get_request_values(url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");

        if hvalue.is_empty() {
            return None;
        }

        HeaderValue::from_maybe_shared(Bytes::from(hvalue)).ok()
    }

    pub(crate) fn store_cookies_for_url<'a>(
        &self,
        url: &Url,
        set_cookie_headers: impl Iterator<Item = &'a HeaderValue>,
    ) {
        let iter = set_cookie_headers.filter_map(|v| match parse_cookie(v.as_bytes()) {
            Ok(c) => Some(c.into_owned()),
            Err(err) => {
                warn!("Invalid cookie could not be stored to jar: {}", err);
                None
            }
        });
        self.0.borrow_mut().store_response_cookies(iter, url)
    }

    /// Remove all the cookies stored in the [CookieJar].
    pub fn clear(&mut self) {
        self.0.borrow_mut().clear();
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
