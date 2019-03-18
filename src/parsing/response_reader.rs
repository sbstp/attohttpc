use std::io::{self, Read, Write};

#[cfg(feature = "charsets")]
use encoding_rs::Encoding;
use http::header::HeaderMap;
#[cfg(feature = "charsets")]
use http::header::CONTENT_TYPE;
#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

#[cfg(feature = "charsets")]
use crate::charsets::{self, Charset};
use crate::error::HttpResult;
#[cfg(feature = "charsets")]
use crate::parsing::buffers::trim_byte;
use crate::parsing::CompressedReader;
use crate::request::PreparedRequest;
#[cfg(feature = "charsets")]
use crate::streams::StreamDecoder;

#[cfg(feature = "charsets")]
fn get_charset(headers: &HeaderMap, default_charset: Option<Charset>) -> Charset {
    if let Some(value) = headers.get(CONTENT_TYPE) {
        let bytes = value.as_bytes();
        if let Some(scol) = bytes.iter().position(|&b| b == b';') {
            let rhs = trim_byte(b' ', &bytes[scol + 1..]);
            if rhs.starts_with(b"charset=") {
                if let Some(enc) = Encoding::for_label(&rhs[8..]) {
                    return enc;
                }
            }
        }
    }
    default_charset.unwrap_or(charsets::WINDOWS_1252)
}

/// The `ResponseReader` is used to read the body of a reponse.
pub struct ResponseReader {
    inner: CompressedReader,
    #[cfg(feature = "charsets")]
    charset: Charset,
}

impl ResponseReader {
    #[cfg(feature = "charsets")]
    pub(crate) fn new(headers: &HeaderMap, request: &PreparedRequest, reader: CompressedReader) -> ResponseReader {
        ResponseReader {
            inner: reader,
            charset: get_charset(&headers, request.default_charset),
        }
    }

    #[cfg(not(feature = "charsets"))]
    pub(crate) fn new(_: &HeaderMap, _: &PreparedRequest, reader: CompressedReader) -> ResponseReader {
        ResponseReader { inner: reader }
    }

    /// Write the response to any object that implements `Write`.
    pub fn write_to<W>(mut self, mut writer: W) -> HttpResult<u64>
    where
        W: Write,
    {
        let n = io::copy(&mut self.inner, &mut writer)?;
        Ok(n)
    }

    /// Read the response to a `Vec` of bytes.
    pub fn bytes(self) -> HttpResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.write_to(&mut buf)?;
        Ok(buf)
    }

    /// Read the response to a `String`.
    ///
    /// The the UTF-8 codec is assumed. Use the `charsets` featured to get more options.
    #[cfg(not(feature = "charsets"))]
    pub fn string(mut self) -> HttpResult<String> {
        let mut contents = String::new();
        self.inner.read_to_string(&mut contents)?;
        Ok(contents)
    }

    /// Read the response to a `String`.
    ///
    /// If the response headers contain charset information, that charset will be used to decode the body.
    /// Otherwise, if a default encoding is set it will be used. If there is no default encoding, ISO-8859-1
    /// will be used.
    #[cfg(feature = "charsets")]
    pub fn string(self) -> HttpResult<String> {
        let charset = self.charset;
        self.string_with(charset)
    }

    /// Read the response to a `String`, decoding with the given `Encoding`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    #[cfg(feature = "charsets")]
    pub fn string_with(self, charset: Charset) -> HttpResult<String> {
        let mut decoder = StreamDecoder::new(charset);
        self.write_to(&mut decoder)?;
        Ok(decoder.take())
    }

    /// Parse the response as a JSON object and return it.
    #[cfg(feature = "json")]
    pub fn json<T>(self) -> HttpResult<T>
    where
        T: DeserializeOwned,
    {
        let text = self.string()?;
        let obj = serde_json::from_str(&text)?;
        Ok(obj)
    }
}

impl Read for ResponseReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

#[cfg(test)]
#[cfg(feature = "charsets")]
mod tests {
    use http::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

    use super::get_charset;
    use crate::charsets;

    #[test]
    fn test_get_charset_from_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_bytes(&b"text/html; charset=UTF-8"[..]).unwrap(),
        );
        assert_eq!(get_charset(&headers, None), charsets::UTF_8);
    }

    #[test]
    fn test_get_charset_from_header_lowercase() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_bytes(&b"text/html; charset=utf8"[..]).unwrap(),
        );
        assert_eq!(get_charset(&headers, None), charsets::UTF_8);
    }

    #[test]
    fn test_get_charset_from_default() {
        let headers = HeaderMap::new();
        assert_eq!(get_charset(&headers, Some(charsets::UTF_8)), charsets::UTF_8);
    }

    #[test]
    fn test_get_charset_standard() {
        let headers = HeaderMap::new();
        assert_eq!(get_charset(&headers, None), charsets::WINDOWS_1252);
    }

}
