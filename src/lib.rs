#![deny(missing_docs)]
//! Check out the [repository](https://github.com/sbstp/lynx) for more general information
//! and examples about this crate.

#[macro_use]
extern crate log;

#[cfg(feature = "charsets")]
pub mod charsets;
mod error;
mod parsing;
mod request;
mod streams;

pub use crate::error::{HttpError, HttpResult};
pub use crate::parsing::ResponseReader;
pub use crate::request::{PreparedRequest, RequestBuilder};
#[cfg(feature = "charsets")]
pub use crate::{charsets::Charset, parsing::TextReader};
pub use http::StatusCode;
pub mod header {
    //! This module is a re-export of the `http` crate's `header` module.
    pub use http::header::*;
}
use http::Method;

/// Create a new `RequestBuilder` with the GET method.
pub fn get<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::GET, base_url)
}

/// Create a new `RequestBuilder` with the POST method.
pub fn post<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::POST, base_url)
}

/// Create a new `RequestBuilder` with the PUT method.
pub fn put<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::PUT, base_url)
}

/// Create a new `RequestBuilder` with the DELETE method.
pub fn delete<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::DELETE, base_url)
}

/// Create a new `RequestBuilder` with the HEAD method.
pub fn head<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::HEAD, base_url)
}

/// Create a new `RequestBuilder` with the OPTIONS method.
pub fn options<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::OPTIONS, base_url)
}

/// Create a new `RequestBuilder` with the PATCH method.
pub fn patch<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::PATCH, base_url)
}

/// Create a new `RequestBuilder` with the TRACE method.
pub fn trace<U>(base_url: U) -> RequestBuilder
where
    U: AsRef<str>,
{
    RequestBuilder::new(Method::TRACE, base_url)
}
