use std::io::{self, Read};

use http::header::{HeaderMap, CONTENT_ENCODING};
#[cfg(feature = "compress")]
use libflate::{deflate, gzip};

use crate::error::{HttpError, HttpResult};
use crate::parsing::body_reader::BodyReader;

pub enum CompressedReader {
    Plain(BodyReader),
    #[cfg(feature = "compress")]
    Deflate(deflate::Decoder<BodyReader>),
    #[cfg(feature = "compress")]
    Gzip(gzip::Decoder<BodyReader>),
}

impl CompressedReader {
    #[cfg(feature = "compress")]
    pub fn new(headers: &HeaderMap, reader: BodyReader) -> HttpResult<CompressedReader> {
        if let Some(content_encoding) = headers.get(CONTENT_ENCODING).map(|v| v.as_bytes()) {
            match content_encoding {
                b"deflate" => Ok(CompressedReader::Deflate(deflate::Decoder::new(reader))),
                b"gzip" => Ok(CompressedReader::Gzip(gzip::Decoder::new(reader)?)),
                _ => Err(HttpError::InvalidResponse("invalid Content-Encoding header")),
            }
        } else {
            Ok(CompressedReader::Plain(reader))
        }
    }

    #[cfg(not(feature = "compress"))]
    pub fn new(headers: &HeaderMap, reader: BodyReader) -> HttpResult<CompressedReader> {
        Ok(CompressedReader::Plain(reader))
    }
}

impl Read for CompressedReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
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

    #[cfg(feature = "compress")]
    use libflate::{deflate, gzip};

    use crate::parsing::response::parse_response;
    use crate::streams::BaseStream;
    use crate::Request;

    #[test]
    fn test_stream_plain() {
        let payload = b"Hello world!!!!!!!!";

        let mut buf: Vec<u8> = Vec::new();
        let _ = write!(buf, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", payload.len());
        buf.extend(payload);

        let req = Request::get("http://google.ca");

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

        let req = Request::get("http://google.ca");

        let sock = BaseStream::mock(buf);
        let (_, _, response) = parse_response(sock, &req).unwrap();
        assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
    }

    #[test]
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

        let req = Request::get("http://google.ca");

        let sock = BaseStream::mock(buf);
        let (_, _, response) = parse_response(sock, &req).unwrap();

        assert_eq!(response.string().unwrap(), "Hello world!!!!!!!!");
    }
}
