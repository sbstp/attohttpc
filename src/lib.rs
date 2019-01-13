#[macro_use]
extern crate log;

pub mod charsets;
mod error;
mod request;
mod tls;

pub use crate::charsets::Charset;
pub use crate::error::{HttpError, HttpResult};
pub use crate::request::parse::ResponseReader;
pub use crate::request::Request;
pub use http::StatusCode;
pub mod header {
    pub use http::header::*;
}
