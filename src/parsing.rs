use std::io;

pub mod body_reader;
pub mod buffers;
pub mod chunked_reader;
pub mod compressed_reader;
pub mod length_reader;
pub mod response;
pub mod response_reader;

pub fn error(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}

pub use self::body_reader::BodyReader;
pub use self::chunked_reader::ChunkedReader;
pub use self::compressed_reader::CompressedReader;
pub use self::length_reader::LengthReader;
pub use self::response::{parse_response, parse_response_head};
pub use self::response_reader::ResponseReader;
