use std::io::{self, BufReader, Read};

use http::header::{HeaderMap, CONTENT_LENGTH, TRANSFER_ENCODING};

use crate::error::{HttpError, HttpResult};
use crate::parsing::{ChunkedReader, LengthReader};
use crate::streams::BaseStream;

pub enum BodyReader {
    Chunked(ChunkedReader<BaseStream>),
    Length(LengthReader<BufReader<BaseStream>>),
    NoBody,
}

impl BodyReader {
    #[inline]
    #[cfg(feature = "compress")]
    pub fn is_no_body(&self) -> bool {
        match self {
            BodyReader::NoBody => true,
            _ => false,
        }
    }
}

impl Read for BodyReader {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            BodyReader::Chunked(r) => r.read(buf),
            BodyReader::Length(r) => r.read(buf),
            BodyReader::NoBody => Ok(0),
        }
    }
}

impl BodyReader {
    pub fn new(headers: &HeaderMap, reader: BufReader<BaseStream>) -> HttpResult<BodyReader> {
        if headers.get(TRANSFER_ENCODING).map(|v| v.as_bytes()) == Some(b"chunked") {
            debug!("creating a chunked body reader");
            Ok(BodyReader::Chunked(ChunkedReader::new(reader)))
        } else if let Some(val) = headers.get(CONTENT_LENGTH) {
            debug!("creating a length body reader");
            let val = val
                .to_str()
                .map_err(|_| HttpError::InvalidResponse("invalid content length: not a string"))?;
            let val: u64 = u64::from_str_radix(val, 10)
                .map_err(|_| HttpError::InvalidResponse("invalid content length: not a number"))?;
            Ok(BodyReader::Length(LengthReader::new(reader, val)))
        } else {
            debug!("creating a no-body body reader");
            Ok(BodyReader::NoBody)
        }
    }
}
