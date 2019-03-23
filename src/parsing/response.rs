use std::io::{BufReader, Read, Write};
use std::str;

use http::{
    header::{HeaderName, HeaderValue, TRANSFER_ENCODING},
    HeaderMap, StatusCode,
};

use crate::error::{Error, Result};
use crate::parsing::buffers::{self, trim_byte};
use crate::parsing::{BodyReader, CompressedReader, ResponseReader};
use crate::request::PreparedRequest;
use crate::streams::BaseStream;

#[cfg(feature = "charsets")]
use crate::{charsets::Charset, parsing::TextReader};

#[cfg(feature = "json")]
use serde::de::DeserializeOwned;

pub fn parse_response_head<R>(reader: &mut BufReader<R>) -> Result<(StatusCode, HeaderMap)>
where
    R: Read,
{
    let mut line = Vec::new();
    let mut headers = HeaderMap::new();

    // status line
    let status: StatusCode = {
        buffers::read_line(reader, &mut line)?;
        let mut parts = line.split(|&b| b == b' ').filter(|x| !x.is_empty());

        let _ = parts.next().ok_or(Error::InvalidResponse("invalid status line"))?;
        let code = parts.next().ok_or(Error::InvalidResponse("invalid status line"))?;

        str::from_utf8(code)
            .map_err(|_| Error::InvalidResponse("cannot decode code"))?
            .parse()
            .map_err(|_| Error::InvalidResponse("invalid status code"))?
    };

    loop {
        buffers::read_line(reader, &mut line)?;
        if line.is_empty() {
            break;
        }

        let col = line
            .iter()
            .position(|&c| c == b':')
            .ok_or(Error::InvalidResponse("header has no colon"))?;

        let header = trim_byte(b' ', &line[..col]);
        let value = trim_byte(b' ', &line[col + 1..]);

        headers.append(
            HeaderName::from_bytes(header).map_err(http::Error::from)?,
            HeaderValue::from_bytes(value).map_err(http::Error::from)?,
        );
    }

    Ok((status, headers))
}

pub fn parse_response(reader: BaseStream, request: &PreparedRequest) -> Result<Response> {
    let mut reader = BufReader::new(reader);
    let (status, mut headers) = parse_response_head(&mut reader)?;
    let body_reader = BodyReader::new(&headers, reader)?;
    let compressed_reader = CompressedReader::new(&headers, request, body_reader)?;
    let response_reader = ResponseReader::new(&headers, request, compressed_reader);

    // Remove HOP-BY-HOP headers
    headers.remove(TRANSFER_ENCODING);

    Ok(Response {
        status,
        headers,
        reader: response_reader,
    })
}

/// `Response` represents a reponse returned by a server.
pub struct Response {
    status: StatusCode,
    headers: HeaderMap,
    reader: ResponseReader,
}

impl Response {
    /// Get the status code of this `Response`.
    #[inline]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Get the headers of this `Response`.
    #[inline]
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Checks if the status code of this `Response` was a success code.
    #[inline]
    pub fn is_success(&self) -> bool {
        self.status.is_success()
    }

    /// Split this `Reponse` into a tuple of `StatusCode`, `HeaderMap`, `ResponseReader`.
    ///
    /// This method is useful to read the status code or headers after consuming the response.
    #[inline]
    pub fn split(self) -> (StatusCode, HeaderMap, ResponseReader) {
        (self.status, self.headers, self.reader)
    }

    /// Write the response to any object that implements `Write`.
    #[inline]
    pub fn write_to<W>(self, writer: W) -> Result<u64>
    where
        W: Write,
    {
        self.reader.write_to(writer)
    }

    /// Read the response to a `Vec` of bytes.
    #[inline]
    pub fn bytes(self) -> Result<Vec<u8>> {
        self.reader.bytes()
    }

    /// Read the response to a `String`.
    ///
    /// The the UTF-8 codec is assumed. Use the `charsets` feature to get more options.
    #[inline]
    pub fn text(self) -> Result<String> {
        self.reader.text()
    }

    /// Read the response to a `String`, decoding with the given `Charset`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    #[inline]
    pub fn text_with(self, charset: Charset) -> Result<String> {
        self.reader.text_with(charset)
    }

    /// Create a `TextReader` from this `ResponseReader`.
    ///
    /// If the response headers contain charset information, that charset will be used to decode the body.
    /// Otherwise, if a default encoding is set it will be used. If there is no default encoding, ISO-8859-1
    /// will be used.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    pub fn text_reader(self) -> TextReader<BufReader<ResponseReader>> {
        self.reader.text_reader()
    }

    /// Create a `TextReader` from this `ResponseReader`, decoding with the given `Charset`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    ///
    /// This method only exists when the `charsets` feature is enabled.
    #[cfg(feature = "charsets")]
    #[inline]
    pub fn text_reader_with(self, charset: Charset) -> TextReader<BufReader<ResponseReader>> {
        self.reader.text_reader_with(charset)
    }

    /// Read the response body to a String using the UTF-8 encoding.
    ///
    /// This method ignores headers and the default encoding.
    #[inline]
    pub fn text_utf8(self) -> Result<String> {
        self.reader.text_utf8()
    }

    /// Parse the response as a JSON object and return it.
    ///
    /// This method will attempt to decode the text using the response headers or the default encoding,
    /// falling back to ISO-8559-1 if they aren't set.
    ///
    /// When the `charsets` feature is disabled, this method can only decode UTF-8 encoded JSON.
    #[cfg(feature = "json")]
    #[inline]
    pub fn json<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.reader.json()
    }

    /// Parse the response as a JSON object encoded in UTF-8.
    ///
    /// This method ignores headers and the default encoding.
    /// This method only exists when the `json` feature is enabled.
    #[cfg(feature = "json")]
    #[inline]
    pub fn json_utf8<T>(self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.reader.json_utf8()
    }
}

#[test]
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let mut reader = BufReader::new(&response[..]);
    let (status, headers) = parse_response_head(&mut reader).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}
