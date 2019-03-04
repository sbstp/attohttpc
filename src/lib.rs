#[macro_use]
extern crate log;

#[cfg(feature = "charsets")]
pub mod charsets;
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
    pub use http::header::*;
}
