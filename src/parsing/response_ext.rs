use http::response::{Parts, Response};

use crate::header::HeaderMap;
use crate::{ErrorKind, Result, StatusCode};

/// An extension trait adding helper methods to `Response`.
pub trait ResponseExt: Sized + sealed::Sealed {
    /// Body type of the response.
    type Body;

    /// Checks if the status code of this `Response` was a success code.
    fn is_success(&self) -> bool;

    /// Returns error variant if the status code was not a success code.
    fn error_for_status(self) -> Result<Self>;

    /// Split this `Response` into a tuple of `StatusCode`, `HeaderMap`, `Self::Body`.
    ///
    /// This method is useful to read the status code or headers after consuming the response.
    fn split(self) -> (StatusCode, HeaderMap, Self::Body);
}

mod sealed {
    pub trait Sealed {}
    impl<B> Sealed for http::Response<B> {}
}

impl<B> ResponseExt for Response<B> {
    type Body = B;

    #[inline]
    fn is_success(&self) -> bool {
        self.status().is_success()
    }

    fn error_for_status(self) -> Result<Self> {
        if self.is_success() {
            Ok(self)
        } else {
            Err(ErrorKind::StatusCode(self.status()).into())
        }
    }

    fn split(self) -> (StatusCode, HeaderMap, Self::Body) {
        let (Parts { status, headers, .. }, body) = self.into_parts();
        (status, headers, body)
    }
}
