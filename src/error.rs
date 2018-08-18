use std::io;
use std::result;
use std::str::Utf8Error;
use std::string::FromUtf8Error;

use http;

#[derive(Fail, Debug)]
pub enum HttpError {
    #[fail(display = "{}", _0)]
    Io(io::Error),

    #[fail(display = "{}", _0)]
    Utf8(Utf8Error),

    #[fail(display = "{}", _0)]
    FromUtf8(FromUtf8Error),

    #[fail(display = "{}", _0)]
    Http(http::Error),

    #[fail(display = "invalid url")]
    InvalidUrl,

    #[fail(display = "invalid protocol")]
    InvalidResponse,
}

macro_rules! impl_from {
    ($t:ty, $i:ident) => {
        impl From<$t> for HttpError {
            fn from(err: $t) -> HttpError {
                HttpError::$i(err)
            }
        }
    };
}

// impl<T> From<T> for HttpError
// where
//     T: Into<http::Error>,
// {
//     fn from(err: T) -> HttpError {
//         HttpError::Http(err.into())
//     }
// }

impl_from!(io::Error, Io);
impl_from!(Utf8Error, Utf8);
impl_from!(FromUtf8Error, FromUtf8);
impl_from!(http::Error, Http);

pub type HttpResult<T = ()> = result::Result<T, HttpError>;
