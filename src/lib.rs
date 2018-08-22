#![feature(nll)]
#![feature(uniform_paths)]

#[macro_use]
extern crate log;

mod error;
mod request;
mod tls;

pub use crate::error::{HttpError, HttpResult};
pub use crate::request::parse::ResponseReader;
pub use crate::request::Request;
pub mod header {
    pub use http::header::*;
}
