#![deny(missing_docs)]
//! Check out the [repository](https://github.com/sbstp/lynx) for more general information
//! and examples about this crate.

#[macro_use]
extern crate log;

#[cfg(feature = "charsets")]
pub mod charsets;
mod chunked;
mod error;
mod request;
mod response;
mod streams;

#[cfg(feature = "charsets")]
pub use crate::charsets::Charset;
pub use crate::error::{HttpError, HttpResult};
pub use crate::request::Request;
pub use crate::response::ResponseReader;
pub use http::StatusCode;
pub mod header {
    //! This module is a re-export of the `http` crate's `header` module.
    pub use http::header::*;
}
