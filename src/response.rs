use std::io::{BufRead, BufReader, Read, Write};
use std::str;

#[cfg(feature = "charsets")]
use encoding_rs::Encoding;
#[cfg(feature = "compress")]
use http::header::CONTENT_ENCODING;
#[cfg(feature = "charsets")]
use http::header::CONTENT_TYPE;
use http::{
    header::{HeaderName, HeaderValue},
    HeaderMap, StatusCode,
};
#[cfg(feature = "compress")]
use libflate::{deflate, gzip};

#[cfg(feature = "charsets")]
use crate::charsets::{self, Charset};
use crate::error::{HttpError, HttpResult};
use crate::request::Request;
#[cfg(feature = "charsets")]
use crate::streams::StreamDecoder;
use crate::streams::{BaseStream, CompressedRead};

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

#[cfg(not(feature = "compress"))]
fn get_content_encoding_stream(_: &HeaderMap, reader: BufReader<BaseStream>) -> HttpResult<CompressedRead> {
    Ok(CompressedRead::Plain(reader))
}

#[cfg(feature = "compress")]
fn get_content_encoding_stream(headers: &HeaderMap, reader: BufReader<BaseStream>) -> HttpResult<CompressedRead> {
    Ok(match headers.get(CONTENT_ENCODING).map(|v| v.as_bytes()) {
        Some(b"deflate") => CompressedRead::Deflate(deflate::Decoder::new(reader)),
        Some(b"gzip") => CompressedRead::Gzip(gzip::Decoder::new(reader)?),
        _ => CompressedRead::Plain(reader),
    })
}

/// The `ResponseReader` is used to read the body of a reponse.
pub struct ResponseReader {
    inner: CompressedRead,
    #[cfg(feature = "charsets")]
    charset: Charset,
}

impl ResponseReader {
    #[allow(unused_variables)]
    fn new(headers: &HeaderMap, request: &Request, reader: BufReader<BaseStream>) -> HttpResult<ResponseReader> {
        Ok(ResponseReader {
            inner: get_content_encoding_stream(&headers, reader)?,
            #[cfg(feature = "charsets")]
            charset: get_charset(&headers, request.default_charset),
        })
    }

    /// Write the response to any object that implements `Write`.
    pub fn write_to<W>(mut self, mut writer: W) -> HttpResult<usize>
    where
        W: Write,
    {
        let mut buf = [0u8; 4096];
        let mut count = 0;
        loop {
            match self.inner.read(&mut buf)? {
                0 => break,
                n => {
                    writer.write_all(&buf[..n])?;
                    count += n;
                }
            }
        }
        Ok(count)
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
}

fn trim_crlf(buf: &[u8]) -> &[u8] {
    unsafe {
        if buf.ends_with(b"\r\n") {
            buf.get_unchecked(..buf.len() - 2)
        } else if buf.ends_with(b"\n") {
            buf.get_unchecked(..buf.len() - 1)
        } else {
            buf
        }
    }
}

fn trim_byte(byte: u8, buf: &[u8]) -> &[u8] {
    trim_byte_left(byte, trim_byte_right(byte, buf))
}

fn trim_byte_left(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.first().map(|&b| b) {
        if b == byte {
            unsafe {
                buf = &buf.get_unchecked(1..);
            }
        } else {
            break;
        }
    }
    buf
}

fn trim_byte_right(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.last().map(|&b| b) {
        if b == byte {
            unsafe {
                buf = &buf.get_unchecked(..buf.len() - 1);
            }
        } else {
            break;
        }
    }
    buf
}

pub fn read_response(reader: BaseStream, request: &Request) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
    let mut reader = BufReader::new(reader);
    let (status, headers) = read_response_head(&mut reader)?;
    let resp_reader = ResponseReader::new(&headers, request, reader)?;
    Ok((status, headers, resp_reader))
}

fn read_response_head<R>(mut reader: R) -> HttpResult<(StatusCode, HeaderMap)>
where
    R: BufRead,
{
    let mut line = Vec::new();

    let mut headers = HeaderMap::new();

    // status line
    let status: StatusCode = {
        reader.read_until(b'\n', &mut line)?;
        let trimmed = trim_crlf(&line);

        let sp_1 = trimmed
            .iter()
            .position(|&c| c == b' ')
            .ok_or(HttpError::InvalidResponse("invalid status line"))?;
        let rest = &trimmed[sp_1 + 1..];
        let sp_2 = rest
            .iter()
            .position(|&c| c == b' ')
            .ok_or(HttpError::InvalidResponse("invalid status line"))?;

        str::from_utf8(&rest[..sp_2])
            .map_err(|_| HttpError::InvalidResponse("cannot decode code"))?
            .parse()
            .map_err(|_| HttpError::InvalidResponse("invalid status code"))?
    };

    // headers
    loop {
        line.clear();
        reader.read_until(b'\n', &mut line)?;
        let trimmed = trim_crlf(&line);
        if trimmed.is_empty() {
            break;
        }
        let col_1 = trimmed
            .iter()
            .position(|&c| c == b':')
            .ok_or(HttpError::InvalidResponse("parse header no colon"))?;
        let header = &trimmed[..col_1];
        let value = trim_byte(b' ', &trimmed[col_1 + 1..]);

        headers.append(
            HeaderName::from_bytes(header).map_err(http::Error::from)?,
            HeaderValue::from_bytes(value).map_err(http::Error::from)?,
        );
    }

    Ok((status, headers))
}

#[test]
fn test_trim_crlf() {
    assert_eq!(trim_crlf(b"hello\r\n"), b"hello");
    assert_eq!(trim_crlf(b"hello\n"), b"hello");
    assert_eq!(trim_crlf(b"hello"), b"hello");
}

#[test]
fn test_trim_byte() {
    assert_eq!(trim_byte(b' ', b"  hello  "), b"hello");
    assert_eq!(trim_byte(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte(b' ', b""), b"");
}

#[test]
fn test_trim_byte_left() {
    assert_eq!(trim_byte_left(b' ', b"  hello"), b"hello");
    assert_eq!(trim_byte_left(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte_left(b' ', b""), b"");
}

#[test]
fn test_trim_byte_right() {
    assert_eq!(trim_byte_right(b' ', b"hello  "), b"hello");
    assert_eq!(trim_byte_right(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte_right(b' ', b""), b"");
}

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
fn test_get_charset_from_default() {
    let headers = HeaderMap::new();
    assert_eq!(get_charset(&headers, Some(charsets::UTF_8)), charsets::UTF_8);
}

#[test]
fn test_get_charset_standard() {
    let headers = HeaderMap::new();
    assert_eq!(get_charset(&headers, None), charsets::WINDOWS_1252);
}

#[test]
fn test_stream_plain() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\n\r\n");
    buff.extend(b"Hello world!!!!!!!!");

    let req = Request::get("http://google.ca");

    let sock = BaseStream::mock(buff);
    let (_, _, response) = read_response(sock, &req).unwrap();
    assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
}

#[test]
fn test_stream_deflate() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\nContent-Encoding: deflate\r\n\r\n".iter());
    let mut enc = deflate::Encoder::new(&mut buff);
    enc.write_all(b"Hello world!!!!!!!!").unwrap();
    enc.finish();

    let req = Request::get("http://google.ca");

    let sock = BaseStream::mock(buff);
    let (_, _, response) = read_response(sock, &req).unwrap();
    assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
}

#[test]
fn test_stream_gzip() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\n".iter());
    let mut enc = gzip::Encoder::new(&mut buff).unwrap();
    enc.write_all(b"Hello world!!!!!!!!").unwrap();
    enc.finish();

    let req = Request::get("http://google.ca");

    let sock = BaseStream::mock(buff);
    let (_, _, response) = read_response(sock, &req).unwrap();

    assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
}

#[test]
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let (status, headers) = read_response_head(&response[..]).ok().unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}
