#[cfg(feature = "charsets")]
use std::io::BufReader;
use std::io::Write;

use http::response::Parts;

#[cfg(feature = "charsets")]
use crate::{charsets::Charset, parsing::TextReader};
#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

use crate::header::HeaderMap;
use crate::{ErrorKind, Response, ResponseReader, Result, StatusCode};

/// An extension trait adding helper methods to [`Response`].
pub trait ResponseExt: Sized + sealed::Sealed {
    /// Checks if the status code of this `Response` was a success code.
    fn is_success(&self) -> bool;

    /// Returns error variant if the status code was not a success code.
    fn error_for_status(self) -> Result<Self>;

    /// Split this `Response` into a tuple of `StatusCode`, `HeaderMap`, `ResponseReader`.
    ///
    /// This method is useful to read the status code or headers after consuming the response.
    fn split(self) -> (StatusCode, HeaderMap, ResponseReader);

    /// Write the response to any object that implements `Write`.
    fn write_to<W>(self, writer: W) -> Result<u64>
    where
        W: Write;

    /// Read the response to a `Vec` of bytes.
    fn bytes(self) -> Result<Vec<u8>>;

    /// Read the response to a `String`.
    ///
    /// If the `charsets` feature is enabled, it will try to decode the response using
    /// the encoding in the headers. If there's no encoding specified in the headers,
    /// it will fall back to the default encoding, and if that's also not specified,
    /// it will fall back to the default of ISO-8859-1.
    ///
    /// If the `charsets` feature is disabled, this method is the same as calling
    /// `text_utf8`.
    ///
    /// Note that both conversions are lossy, i.e. they will not raise errors when
    /// invalid data is encountered but output replacement characters instead.
    fn text(self) -> Result<String>;

    /// Read the response to a `String`, decoding with the given `Charset`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    fn text_with(self, charset: Charset) -> Result<String>;

    /// Create a `TextReader` from this `ResponseReader`.
    ///
    /// If the response headers contain charset information, that charset will be used to decode the body.
    /// Otherwise, if a default encoding is set it will be used. If there is no default encoding, ISO-8859-1
    /// will be used.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    fn text_reader(self) -> TextReader<BufReader<ResponseReader>>;

    /// Create a `TextReader` from this `ResponseReader`, decoding with the given `Charset`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    fn text_reader_with(self, charset: Charset) -> TextReader<BufReader<ResponseReader>>;

    /// Read the response body to a String using the UTF-8 encoding.
    ///
    /// This method ignores headers and the default encoding.
    ///
    /// Note that is lossy, i.e. it will not raise errors when
    /// invalid data is encountered but output replacement characters instead.
    fn text_utf8(self) -> Result<String>;

    /// Parse the response as a JSON object and return it.
    ///
    /// If the `charsets` feature is enabled, it will try to decode the response using
    /// the encoding in the headers. If there's no encoding specified in the headers,
    /// it will fall back to the default encoding, and if that's also not specified,
    /// it will fall back to the default of ISO-8859-1.
    ///
    /// If the `charsets` feature is disabled, this method is the same as calling
    /// `json_utf8`.
    #[cfg(feature = "json")]
    fn json<T>(self) -> Result<T>
    where
        T: DeserializeOwned;

    /// Parse the response as a JSON object encoded in UTF-8.
    ///
    /// This method ignores headers and the default encoding.
    ///
    /// This method only exists when the `json` feature is enabled.
    #[cfg(feature = "json")]
    fn json_utf8<T>(self) -> Result<T>
    where
        T: DeserializeOwned;
}

mod sealed {
    use crate::Response;

    pub trait Sealed {}
    impl Sealed for Response {}
}

impl ResponseExt for Response {
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

    #[inline]
    fn split(self) -> (StatusCode, HeaderMap, ResponseReader) {
        let (Parts { status, headers, .. }, body) = self.into_parts();
        (status, headers, body)
    }

    #[inline]
    fn write_to<W>(self, writer: W) -> Result<u64>
    where
        W: Write,
    {
        self.into_body().write_to(writer)
    }

    #[inline]
    fn bytes(self) -> Result<Vec<u8>> {
        self.into_body().bytes()
    }

    #[inline]
    fn text(self) -> Result<String> {
        self.into_body().text()
    }

    #[cfg(feature = "charsets")]
    #[inline]
    fn text_with(self, charset: Charset) -> Result<String> {
        self.into_body().text_with(charset)
    }

    #[cfg(feature = "charsets")]
    fn text_reader(self) -> TextReader<BufReader<ResponseReader>> {
        self.into_body().text_reader()
    }

    #[cfg(feature = "charsets")]
    #[inline]
    fn text_reader_with(self, charset: Charset) -> TextReader<BufReader<ResponseReader>> {
        self.into_body().text_reader_with(charset)
    }

    #[inline]
    fn text_utf8(self) -> Result<String> {
        self.into_body().text_utf8()
    }

    #[cfg(feature = "json")]
    #[inline]
    fn json<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.into_body().json()
    }

    #[cfg(feature = "json")]
    #[inline]
    fn json_utf8<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.into_body().json_utf8()
    }
}
