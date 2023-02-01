use std::{cell::RefCell, fmt::Write, rc::Rc};

use bytes::Bytes;
use cookie::Cookie;
use cookie_store::CookieStore;
use url::Url;

use crate::header::HeaderValue;

pub trait IntoCookie {
    fn into_cookie(self) -> Cookie<'static>;
}

impl<T1, T2> IntoCookie for (T1, T2)
where
    T1: Into<String>,
    T2: Into<String>,
{
    fn into_cookie(self) -> Cookie<'static> {
        Cookie::build(self.0.into(), self.1.into()).finish()
    }
}

impl<'a> IntoCookie for Cookie<'a> {
    fn into_cookie(self) -> Cookie<'static> {
        self.into_owned()
    }
}

impl<'a> IntoCookie for cookie::CookieBuilder<'a> {
    fn into_cookie(self) -> Cookie<'static> {
        self.finish().into_owned()
    }
}

/// Persists cookies between requests.
///
/// All the typical cookie properties, such as expiry, secure and http-only are respected.
#[derive(Clone, Debug)]
pub struct CookieJar(Rc<RefCell<CookieStore>>);

impl CookieJar {
    pub(crate) fn new() -> Self {
        CookieJar(Rc::new(RefCell::new(CookieStore::default())))
    }

    ///
    pub fn cookies_for_url(&self, url: &Url) -> Vec<(String, String)> {
        self.0
            .borrow()
            .get_request_values(url)
            .map(|(name, value)| (name.into(), value.into()))
            .collect()
    }

    ///
    pub fn store_cookie_for_url(&self, cookie: impl IntoCookie, url: &Url) {
        self.0
            .borrow_mut()
            .store_response_cookies(Some(cookie.into_cookie()).into_iter(), url)
    }

    pub(crate) fn header_value_for_url(&self, url: &Url) -> Option<HeaderValue> {
        // let hvalue = self
        //     .0
        //     .borrow()
        //     .get_request_values(url)
        //     .map(|(name, value)| format!("{name}={value}"))
        //     .collect::<Vec<_>>()
        //     .join("; ");

        let mut hvalue = String::new();
        for (idx, (name, value)) in self.0.borrow().get_request_values(url).enumerate() {
            if idx > 0 {
                hvalue.push_str("; ");
            }
            write!(hvalue, "{name}={value}").unwrap();
        }

        if hvalue.is_empty() {
            return None;
        }

        HeaderValue::from_maybe_shared(Bytes::from(hvalue)).ok()
    }

    pub(crate) fn store_cookies_raw_for_url<'a>(
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

fn parse_cookie(buf: &[u8]) -> Result<Cookie, Box<dyn std::error::Error>> {
    let s = std::str::from_utf8(buf)?;
    let c = Cookie::parse(s)?;
    Ok(c)
}

impl Default for CookieJar {
    fn default() -> Self {
        Self::new()
    }
}

#[test]
fn test_header_value() {
    let url = Url::parse("http://example.com").expect("invalid url");
    let jar = CookieJar::new();
    jar.store_cookie_for_url(("foo", "bar"), &url);
    jar.store_cookie_for_url(("qux", "baz"), &url);

    let val = jar.header_value_for_url(&url).unwrap();
    assert_eq!(val.as_bytes(), b"qux=baz; foo=bar");
}
