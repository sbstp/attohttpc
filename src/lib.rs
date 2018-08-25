#![feature(nll)]
#![feature(uniform_paths)]
//#![warn(missing_docs)]

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
pub mod header {
    pub use http::header::*;
}
