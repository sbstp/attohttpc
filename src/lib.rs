#![feature(nll)]
#![feature(rust_2018_preview, uniform_paths)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate log;

mod error;
mod request;
mod tls;

pub use error::{HttpError, HttpResult};
pub use request::parse::ResponseReader;
pub use request::Request;
pub mod header {
    pub use http::header::*;
}
