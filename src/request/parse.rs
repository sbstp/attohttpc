use std::io::{self, BufRead, BufReader, Read, Write};
use std::str;

use encoding_rs::{CoderResult, Decoder, Encoding};
use http::header::{CONTENT_ENCODING, CONTENT_TYPE};

use http::{
    header::{HeaderName, HeaderValue},
    HeaderMap, StatusCode,
};
use libflate::{deflate, gzip};

use crate::error::{HttpError, HttpResult};
use crate::tls::MaybeTls;

enum MaybeCompressed {
    Plain(BufReader<MaybeTls>),
    // TODO: perhaps fix this as there's double buffering between the BufReader and the Decoder.
    // Issue is that the BufReader contains some data that we can't put back in the socket. We'd have
    // to drain the BufRead into the Decoder somehow.
    Deflate(deflate::Decoder<BufReader<MaybeTls>>),
    Gzip(gzip::Decoder<BufReader<MaybeTls>>),
}

impl Read for MaybeCompressed {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            MaybeCompressed::Plain(r) => r.read(buf),
            MaybeCompressed::Deflate(r) => r.read(buf),
            MaybeCompressed::Gzip(r) => r.read(buf),
        }
    }
}

pub struct ResponseReader {
    inner: MaybeCompressed,
    encoding: &'static Encoding,
}

impl ResponseReader {
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
    /// If the response headers contain charset information, that charset will be used to decode the body.
    /// Otherwise, if a default encoding is set it will be used. If there is no default encoding, ISO-8859-1
    /// will be used.
    pub fn string(self) -> HttpResult<String> {
        let encoding = self.encoding;
        self.decode(encoding)
    }

    /// Read the response to a `String`, decoding with the given `Encoding`.
    ///
    /// This will ignore the encoding from the response headers and the default encoding, if any.
    pub fn decode(self, encoding: &'static Encoding) -> HttpResult<String> {
        let mut decoder = StreamDecoder::new(encoding);
        self.write_to(&mut decoder)?;
        Ok(decoder.take())
    }
}

struct StreamDecoder {
    output: String,
    decoder: Decoder,
}

impl StreamDecoder {
    fn new(encoding: &'static Encoding) -> StreamDecoder {
        StreamDecoder {
            output: String::with_capacity(1024),
            decoder: encoding.new_decoder(),
        }
    }

    fn take(mut self) -> String {
        self.decoder.decode_to_string(&[], &mut self.output, true);
        self.output
    }
}

impl Write for StreamDecoder {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let len = buf.len();
        while buf.len() > 0 {
            match self.decoder.decode_to_string(&buf, &mut self.output, false) {
                (CoderResult::InputEmpty, written, _) => {
                    buf = &buf[written..];
                }
                (CoderResult::OutputFull, written, _) => {
                    buf = &buf[written..];
                    self.output.reserve(self.output.capacity());
                }
            }
        }
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
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

fn get_charset(
    headers: &HeaderMap,
    default_encoding: Option<&'static Encoding>,
) -> &'static Encoding {
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
    default_encoding.unwrap_or(encoding_rs::WINDOWS_1252)
}

fn get_content_encoding_stream(
    headers: &HeaderMap,
    reader: BufReader<MaybeTls>,
) -> HttpResult<MaybeCompressed> {
    Ok(match headers.get(CONTENT_ENCODING).map(|v| v.as_bytes()) {
        Some(b"deflate") => MaybeCompressed::Deflate(deflate::Decoder::new(reader)),
        Some(b"gzip") => MaybeCompressed::Gzip(gzip::Decoder::new(reader)?),
        _ => MaybeCompressed::Plain(reader),
    })
}

pub fn read_response(
    reader: MaybeTls,
    default_encoding: Option<&'static Encoding>,
) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
    let mut reader = BufReader::new(reader);
    let (status, headers) = read_response_head(&mut reader)?;
    let encoding = get_charset(&headers, default_encoding);
    let stream = get_content_encoding_stream(&headers, reader)?;
    Ok((
        status,
        headers,
        ResponseReader {
            inner: stream,
            encoding,
        },
    ))
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
    assert_eq!(get_charset(&headers, None), encoding_rs::UTF_8);
}

#[test]
fn test_get_charset_from_default() {
    let headers = HeaderMap::new();
    assert_eq!(
        get_charset(&headers, Some(encoding_rs::UTF_8)),
        encoding_rs::UTF_8
    );
}

#[test]
fn test_get_charset_standard() {
    let headers = HeaderMap::new();
    assert_eq!(get_charset(&headers, None), encoding_rs::WINDOWS_1252);
}

#[test]
fn test_stream_decoder_utf8() {
    let mut decoder = StreamDecoder::new(encoding_rs::UTF_8);
    decoder.write_all("québec".as_bytes()).unwrap();
    assert_eq!(decoder.take(), "québec");
}

#[test]
fn test_stream_decoder_latin1() {
    let mut decoder = StreamDecoder::new(encoding_rs::WINDOWS_1252);
    decoder.write_all(&[201]).unwrap();
    assert_eq!(decoder.take(), "É");
}

#[test]
fn test_stream_decoder_large_buffer() {
    let mut decoder = StreamDecoder::new(encoding_rs::WINDOWS_1252);
    let mut buf = vec![];
    for _ in 0..10_000 {
        buf.push(201);
    }
    decoder.write_all(&buf).unwrap();
    for c in decoder.take().chars() {
        assert_eq!(c, 'É');
    }
}

#[test]
fn test_stream_plain() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\n\r\n");
    buff.extend(b"Hello world!!!!!!!!");

    let sock = MaybeTls::mock(buff);
    let (_, _, response) = read_response(sock, None).unwrap();
    assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
}

#[test]
fn test_stream_deflate() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\nContent-Encoding: deflate\r\n\r\n".iter());
    let mut enc = deflate::Encoder::new(&mut buff);
    enc.write_all(b"Hello world!!!!!!!!").unwrap();
    enc.finish();

    let sock = MaybeTls::mock(buff);
    let (_, _, response) = read_response(sock, None).unwrap();
    assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
}

#[test]
fn test_stream_gzip() {
    let mut buff: Vec<u8> = Vec::new();
    buff.extend(b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\n\r\n".iter());
    let mut enc = gzip::Encoder::new(&mut buff).unwrap();
    enc.write_all(b"Hello world!!!!!!!!").unwrap();
    enc.finish();

    let sock = MaybeTls::mock(buff);
    let (_, _, response) = read_response(sock, None).unwrap();
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
