use std::io::Read;
use std::str;

use http::{
    header::{HeaderName, HeaderValue},
    HeaderMap, StatusCode,
};

use crate::error::{HttpError, HttpResult};
use crate::parsing::buffers::trim_byte;
use crate::parsing::{BodyReader, CompressedReader, ExpandingBufReader, ResponseReader};
use crate::request::Request;
use crate::streams::BaseStream;

pub fn parse_response_head<R>(reader: &mut ExpandingBufReader<R>) -> HttpResult<(StatusCode, HeaderMap)>
where
    R: Read,
{
    // status line
    let status: StatusCode = {
        let line = reader.read_line()?;
        let mut parts = line.split(|&b| b == b' ').filter(|x| !x.is_empty());

        let _ = parts.next().ok_or(HttpError::InvalidResponse("invalid status line"))?;
        let code = parts.next().ok_or(HttpError::InvalidResponse("invalid status line"))?;

        str::from_utf8(code)
            .map_err(|_| HttpError::InvalidResponse("cannot decode code"))?
            .parse()
            .map_err(|_| HttpError::InvalidResponse("invalid status code"))?
    };

    let mut headers = HeaderMap::new();

    loop {
        let line = reader.read_line()?;
        if line.is_empty() {
            break;
        }

        let col = line
            .iter()
            .position(|&c| c == b':')
            .ok_or(HttpError::InvalidResponse("header has no colon"))?;

        let header = trim_byte(b' ', &line[..col]);
        let value = trim_byte(b' ', &line[col + 1..]);

        headers.append(
            HeaderName::from_bytes(header).map_err(http::Error::from)?,
            HeaderValue::from_bytes(value).map_err(http::Error::from)?,
        );
    }

    Ok((status, headers))
}

pub fn parse_response(reader: BaseStream, request: &Request) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
    let mut reader = ExpandingBufReader::new(reader);
    let (status, headers) = parse_response_head(&mut reader)?;
    let body_reader = BodyReader::new(&headers, reader)?;
    let compressed_reader = CompressedReader::new(&headers, body_reader)?;
    let response_reader = ResponseReader::new(&headers, request, compressed_reader);
    Ok((status, headers, response_reader))
}

#[test]
fn test_read_request_head() {
    let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\nhello";
    let mut reader = ExpandingBufReader::new(&response[..]);
    let (status, headers) = parse_response_head(&mut reader).unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[http::header::CONTENT_LENGTH], "5");
    assert_eq!(headers[http::header::CONTENT_TYPE], "text/plain");
}
