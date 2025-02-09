use std::io::{BufReader, Read};
use std::str;

use http::{
    header::{HeaderName, HeaderValue, TRANSFER_ENCODING},
    HeaderMap, StatusCode,
};

use crate::error::{InvalidResponseKind, Result};
use crate::parsing::buffers::{self, trim_byte};
use crate::parsing::{body_reader::BodyReader, compressed_reader::CompressedReader, ResponseReader};
use crate::request::PreparedRequest;
use crate::streams::BaseStream;

/// `Response` represents a response returned by a server.
pub type Response = http::Response<ResponseReader>;

pub fn parse_response_head<R>(reader: &mut BufReader<R>, max_headers: usize) -> Result<(StatusCode, HeaderMap)>
where
    R: Read,
{
    const MAX_LINE_LEN: u64 = 16 * 1024;

    let mut line = Vec::new();
    let mut headers = HeaderMap::new();

    // status line
    let status: StatusCode = {
        buffers::read_line(reader, &mut line, MAX_LINE_LEN)?;
        let mut parts = line.split(|&b| b == b' ').filter(|x| !x.is_empty());

        let _ = parts.next().ok_or(InvalidResponseKind::StatusLine)?;
        let code = parts.next().ok_or(InvalidResponseKind::StatusLine)?;

        str::from_utf8(code)
            .map_err(|_| InvalidResponseKind::StatusCode)?
            .parse()
            .map_err(|_| InvalidResponseKind::StatusCode)?
    };

    // headers
    loop {
        buffers::read_line_strict(reader, &mut line, MAX_LINE_LEN)?;
        if line.is_empty() {
            break;
        } else if headers.len() == max_headers {
            return Err(InvalidResponseKind::Header.into());
        }

        let col = line
            .iter()
            .position(|&c| c == b':')
            .ok_or(InvalidResponseKind::Header)?;

        buffers::replace_byte(b'\n', b' ', &mut line[col + 1..]);

        let header = trim_byte(b' ', &line[..col]);
        let value = trim_byte(b' ', &line[col + 1..]);

        let header = match HeaderName::from_bytes(header) {
            Ok(val) => val,
            Err(err) => {
                warn!("Dropped invalid response header: {}", err);
                continue;
            }
        };

        headers.append(header, HeaderValue::from_bytes(value).map_err(http::Error::from)?);
    }

    Ok((status, headers))
}

pub fn parse_response<B>(reader: BaseStream, request: &PreparedRequest<B>) -> Result<Response> {
    let mut reader = BufReader::new(reader);
    let (status, mut headers) = parse_response_head(&mut reader, request.base_settings.max_headers)?;
    let body_reader = BodyReader::new(&headers, reader)?;
    let compressed_reader = CompressedReader::new(&headers, request, body_reader)?;
    let response_reader = ResponseReader::new(&headers, request, compressed_reader);

    // Remove HOP-BY-HOP headers
    headers.remove(TRANSFER_ENCODING);

    let mut response = http::Response::new(response_reader);
    *response.status_mut() = status;
    *response.headers_mut() = headers;

    Ok(response)
}

#[cfg(test)]
use crate::ErrorKind;

#[test]
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let mut reader = BufReader::new(&response[..]);
    let (status, headers) = parse_response_head(&mut reader, 100).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}

#[test]
fn test_line_folded_header() {
    let response = b"HTTP/1.1 200 OK\r\nheader-of-great-many-lines: foo\nbar\nbaz\nqux\r\nthe-other-kind-of-header: foobar\r\n\r\n";
    let mut reader = BufReader::new(&response[..]);
    let (status, headers) = parse_response_head(&mut reader, 100).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers["header-of-great-many-lines"], "foo bar baz qux");
    assert_eq!(headers["the-other-kind-of-header"], "foobar");
}

#[test]
fn test_max_headers_limit() {
    let response = b"HTTP/1.1 200 OK\r\nfirst-header: foo\r\nsecond-header: bar\r\none-header-too-many: baz\r\n\r\n";
    let mut reader = BufReader::new(&response[..]);
    let err = parse_response_head(&mut reader, 2).unwrap_err();
    assert!(matches!(
        err.kind(),
        ErrorKind::InvalidResponse(InvalidResponseKind::Header)
    ));
}
