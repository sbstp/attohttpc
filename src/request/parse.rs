use std::io::{BufRead, BufReader, Read, Write};
use std::str;

use http::{
    header::{HeaderName, HeaderValue},
    HeaderMap, StatusCode,
};

use crate::error::{HttpError, HttpResult};
use crate::tls::MaybeTls;

pub struct ResponseReader {
    inner: BufReader<MaybeTls>,
}

impl ResponseReader {
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

    pub fn bytes(self) -> HttpResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.write_to(&mut buf)?;
        Ok(buf)
    }

    pub fn string(self) -> HttpResult<String> {
        let buf = self.bytes()?;
        Ok(String::from_utf8(buf)?)
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

pub fn read_response(reader: MaybeTls) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
    let mut reader = BufReader::new(reader);
    let (status, headers) = read_response_head(&mut reader)?;
    Ok((status, headers, ResponseReader { inner: reader }))
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
            .ok_or(HttpError::InvalidResponse)?;
        let rest = &trimmed[sp_1 + 1..];
        let sp_2 = rest
            .iter()
            .position(|&c| c == b' ')
            .ok_or(HttpError::InvalidResponse)?;

        str::from_utf8(&rest[..sp_2])?
            .parse()
            .map_err(http::Error::from)?
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
            .ok_or(HttpError::InvalidResponse)?;
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
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let (status, headers) = read_response_head(&response[..]).ok().unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}
