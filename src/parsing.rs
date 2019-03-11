use std::io;

pub mod buffers;
pub mod chunked_reader;
pub mod headers;
pub mod length_reader;

pub fn error(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::Other, msg)
}

pub use self::buffers::ExpandingBufReader;
