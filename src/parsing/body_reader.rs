use std::io::{self, Read};

use http::header::{HeaderMap, CONTENT_LENGTH, TRANSFER_ENCODING};

use crate::error::{HttpError, HttpResult};
use crate::parsing::{ChunkedReader, ExpandingBufReader, LengthReader};
use crate::streams::BaseStream;

pub enum BodyReader {
    Chunked(ChunkedReader<BaseStream>),
    Length(LengthReader<ExpandingBufReader<BaseStream>>),
}

impl Read for BodyReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            BodyReader::Chunked(r) => r.read(buf),
            BodyReader::Length(r) => r.read(buf),
        }
    }
}

impl BodyReader {
    pub fn new(headers: &HeaderMap, reader: ExpandingBufReader<BaseStream>) -> HttpResult<BodyReader> {
        if headers.get(TRANSFER_ENCODING).map(|v| v.as_bytes()) == Some(b"chunked") {
            Ok(BodyReader::Chunked(ChunkedReader::new(reader)))
        } else if let Some(val) = headers.get(CONTENT_LENGTH) {
            let val = val
                .to_str()
                .map_err(|_| HttpError::InvalidResponse("invalid content length: not a string"))?;
            let val: u64 = u64::from_str_radix(val, 10)
                .map_err(|_| HttpError::InvalidResponse("invalid content length: not a number"))?;
            Ok(BodyReader::Length(LengthReader::new(reader, val)))
        } else {
            Err(HttpError::InvalidResponse(
                "no content-length or chunked transfer encoding",
            ))
        }
    }
}
