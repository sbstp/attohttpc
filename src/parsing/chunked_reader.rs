use std::cmp;
use std::io::{self, Read};
use std::u64;

use crate::parsing::ExpandingBufReader;

pub struct ChunkedReader<R>
where
    R: Read,
{
    inner: ExpandingBufReader<R>,
    is_waiting: bool, // is waiting for new chunk
    read: u64,        // bytes read in the chunk
    length: u64,      // chunk length
}

impl<R> ChunkedReader<R>
where
    R: Read,
{
    pub fn new(reader: ExpandingBufReader<R>) -> ChunkedReader<R> {
        ChunkedReader {
            inner: reader,
            is_waiting: true,
            read: 0,
            length: 0,
        }
    }
}

impl<R> Read for ChunkedReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.is_waiting {
            // If we're waiting for a new chunk, we read a line and parse the number as hexadecimal.
            self.read = 0;
            self.length = self.inner.read_line_hex()?;
            // If the chunk's length is 0, we've received the EOF chunk.
            if self.length == 0 {
                // Read CRLF
                let buf = self.inner.read_line()?;
                debug_assert!(buf.len() == 0);
            }
            self.is_waiting = false;
        }

        // If we have a length of 0 for a chunk, we've reached EOF.
        if self.length == 0 {
            return Ok(0);
        }

        // We read the smallest amount between the given buffer's length and the remaining bytes' length.
        let remaining = self.length - self.read;
        let count = cmp::min(remaining, buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..count])?;
        self.read += n as u64;

        // Check for an unexpected EOF
        if n == 0 {
            self.length = 0;
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        // Check if we've read the entire chunk.
        if self.read == self.length {
            // Read CRLF and expect a chunk in the next read.
            let buf = self.inner.read_line()?;
            debug_assert!(buf.len() == 0);
            self.is_waiting = true;
        }

        Ok(n)
    }
}

#[test]
fn test_read_works() {
    let msg = b"4\r\nwiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    reader.read_to_string(&mut s).unwrap();
    assert_eq!(s, "wikipedia in\r\n\r\nchunks.");
}

#[test]
fn test_read_empty() {
    let msg = b"0\r\n\r\n";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    reader.read_to_string(&mut s).unwrap();
    assert_eq!(s, "");
}

#[test]
fn test_read_invalid_empty() {
    let msg = b"";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    assert!(reader.read_to_string(&mut s).is_err());
}

#[test]
fn test_read_invalid_chunk() {
    let msg = b"4\r\nwik";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn test_read_invalid_no_terminating_chunk() {
    let msg = b"4\r\nwiki";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn test_read_invalid_bad_terminating_chunk() {
    let msg = b"4\r\nwiki\r\n0\r\n";
    let mut reader = ChunkedReader::new(ExpandingBufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
}
