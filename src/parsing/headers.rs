use std::io::Read;
use std::str;

use http::{
    header::{HeaderName, HeaderValue},
    HeaderMap, StatusCode,
};

use crate::error::{HttpError, HttpResult};
use crate::parsing::buffers::trim_byte;
use crate::parsing::ExpandingBufReader;

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
