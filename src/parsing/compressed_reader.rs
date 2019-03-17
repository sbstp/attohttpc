#[cfg(feature = "compress")]
use std::io::BufReader;
use std::io::{self, Read};

use http::header::HeaderMap;
#[cfg(feature = "compress")]
use http::header::CONTENT_ENCODING;
#[cfg(feature = "compress")]
use libflate::{deflate, gzip};

#[cfg(feature = "compress")]
use crate::error::HttpError;
use crate::error::HttpResult;
use crate::parsing::body_reader::BodyReader;

pub enum CompressedReader {
    Plain(BodyReader),
    #[cfg(feature = "compress")]
    // The BodyReader needs to be wrapped in a BufReader because libflate reads one byte at a time.
    Deflate(deflate::Decoder<BufReader<BodyReader>>),
    #[cfg(feature = "compress")]
    // The BodyReader needs to be wrapped in a BufReader because libflate reads one byte at a time.
    Gzip(gzip::Decoder<BufReader<BodyReader>>),
}

impl CompressedReader {
    #[cfg(feature = "compress")]
    pub fn new(headers: &HeaderMap, reader: BodyReader) -> HttpResult<CompressedReader> {
        // If there is no body, we must not try to create a compressed reader because gzip tries to read
        // the gzip header and the NoBody reader returns EOF.
        if !reader.is_no_body() {
            if let Some(content_encoding) = headers.get(CONTENT_ENCODING).map(|v| v.as_bytes()) {
                debug!("creating compressed reader from content encoding");
                return match content_encoding {
                    b"deflate" => Ok(CompressedReader::Deflate(deflate::Decoder::new(BufReader::new(reader)))),
                    b"gzip" => Ok(CompressedReader::Gzip(gzip::Decoder::new(BufReader::new(reader))?)),
                    b"identity" => Ok(CompressedReader::Plain(reader)),
                    _ => Err(HttpError::InvalidResponse("invalid Content-Encoding header")),
                };
            }
        }
        debug!("creating plain reader");
        return Ok(CompressedReader::Plain(reader));
    }

    #[cfg(not(feature = "compress"))]
    pub fn new(_: &HeaderMap, reader: BodyReader) -> HttpResult<CompressedReader> {
        Ok(CompressedReader::Plain(reader))
    }
}

impl Read for CompressedReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // TODO: gzip does not read until EOF, leaving some data in the buffer.
        match self {
            CompressedReader::Plain(s) => s.read(buf),
            #[cfg(feature = "compress")]
            CompressedReader::Deflate(s) => s.read(buf),
            #[cfg(feature = "compress")]
            CompressedReader::Gzip(s) => s.read(buf),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::prelude::*;

    use http::Method;
    #[cfg(feature = "compress")]
    use libflate::{deflate, gzip};

    use crate::parsing::response::parse_response;
    use crate::streams::BaseStream;
    use crate::PreparedRequest;

    #[test]
    fn test_stream_plain() {
        let payload = b"Hello world!!!!!!!!";

        let mut buf: Vec<u8> = Vec::new();
        let _ = write!(buf, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", payload.len());
        buf.extend(payload);

        let req = PreparedRequest::new(Method::GET, "http://google.ca");

        let sock = BaseStream::mock(buf);
        let (_, _, response) = parse_response(sock, &req).unwrap();
        assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
    }

    #[test]
    #[cfg(feature = "compress")]
    fn test_stream_deflate() {
        let mut payload = Vec::new();
        let mut enc = deflate::Encoder::new(&mut payload);
        enc.write_all(b"Hello world!!!!!!!!").unwrap();
        enc.finish();

        let mut buf: Vec<u8> = Vec::new();
        let _ = write!(
            buf,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Encoding: deflate\r\n\r\n",
            payload.len()
        );
        buf.extend(payload);

        let req = PreparedRequest::new(Method::GET, "http://google.ca");

        let sock = BaseStream::mock(buf);
        let (_, _, response) = parse_response(sock, &req).unwrap();
        assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
    }

    #[test]
    #[cfg(feature = "compress")]
    fn test_stream_gzip() {
        let mut payload = Vec::new();
        let mut enc = gzip::Encoder::new(&mut payload).unwrap();
        enc.write_all(b"Hello world!!!!!!!!").unwrap();
        enc.finish();

        let mut buf: Vec<u8> = Vec::new();
        let _ = write!(
            buf,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Encoding: gzip\r\n\r\n",
            payload.len()
        );
        buf.extend(payload);

        let req = PreparedRequest::new(Method::GET, "http://google.ca");

        let sock = BaseStream::mock(buf);
        let (_, _, response) = parse_response(sock, &req).unwrap();

        assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
    }
}
