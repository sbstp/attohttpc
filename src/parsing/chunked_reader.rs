use std::cmp;
use std::io::{self, BufReader, Read};
use std::str;
use std::u64;

use crate::parsing::buffers;
use crate::parsing::error;

fn parse_chunk_size(line: &[u8]) -> io::Result<u64> {
    line.iter()
        .position(|&b| b == b';')
        .map_or_else(|| str::from_utf8(line), |idx| str::from_utf8(&line[..idx]))
        .map_err(|_| error("cannot decode chunk size as utf-8"))
        .and_then(|line| u64::from_str_radix(line, 16).map_err(|_| error("cannot decode chunk size as hex")))
}

pub struct ChunkedReader<R>
where
    R: Read,
{
    inner: BufReader<R>,
    is_expecting_chunk: bool, // is waiting for new chunk
    read: u64,                // bytes read in the chunk
    length: u64,              // chunk length
    line: Vec<u8>,
}

impl<R> ChunkedReader<R>
where
    R: Read,
{
    pub fn new(reader: BufReader<R>) -> ChunkedReader<R> {
        ChunkedReader {
            inner: reader,
            is_expecting_chunk: true,
            read: 0,
            length: 0,
            line: Vec::new(),
        }
    }

    #[inline]
    fn remaining(&self) -> u64 {
        self.length - self.read
    }

    #[inline]
    fn read_line(&mut self) -> io::Result<usize> {
        buffers::read_line(&mut self.inner, &mut self.line)
    }

    fn read_chunk_size(&mut self) -> io::Result<u64> {
        self.read_line()?;
        if self.line.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        parse_chunk_size(&self.line)
    }

    fn read_empty_line(&mut self) -> io::Result<()> {
        let n = self.read_line()?;
        if n == 0 || !self.line.is_empty() {
            Err(error("invalid chunk, error in chunked encoding"))
        } else {
            Ok(())
        }
    }
}

impl<R> Read for ChunkedReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.is_expecting_chunk {
            debug!("waiting, parsing new chunk size");
            // If we're waiting for a new chunk, we read a line and parse the number as hexadecimal.
            self.read = 0;
            self.length = self.read_chunk_size()?;
            // If the chunk's length is 0, we've received the EOF chunk.
            if self.length == 0 {
                debug!("received EOF chunk");
                // Read CRLF
                self.read_empty_line()?;
            }
            self.is_expecting_chunk = false;
        }

        // If we have a length of 0 for a chunk, we've reached EOF.
        if self.length == 0 {
            return Ok(0);
        }

        // We read the smallest amount between the given buffer's length and the remaining bytes' length.
        let count = cmp::min(self.remaining(), buf.len() as u64) as usize;

        debug!(
            "before read, remaining={}, count={}, buflen={}",
            self.remaining(),
            count,
            buf.len()
        );

        let n = self.inner.read(&mut buf[..count])?;
        self.read += n as u64;

        debug!("read {} bytes", n);

        // Check for an unexpected EOF
        if n == 0 {
            self.length = 0;
            return Err(io::ErrorKind::UnexpectedEof.into());
        }

        // Check if we've read the entire chunk.
        if self.remaining() == 0 {
            debug!("chunk is finished, expect chunk size in next read call");
            // Read CRLF
            self.read_empty_line()?;
            // Expect a chunk in the next read.
            self.is_expecting_chunk = true;
        }

        Ok(n)
    }
}

#[test]
fn test_read_works() {
    let msg = b"4\r\nwiki\r\n5\r\npedia\r\nE\r\n in\r\n\r\nchunks.\r\n0\r\n\r\n";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    reader.read_to_string(&mut s).unwrap();
    assert_eq!(s, "wikipedia in\r\n\r\nchunks.");
}

#[test]
fn test_read_empty() {
    let msg = b"0\r\n\r\n";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    reader.read_to_string(&mut s).unwrap();
    assert_eq!(s, "");
}

#[test]
fn test_read_invalid_empty() {
    let msg = b"";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    assert!(reader.read_to_string(&mut s).is_err());
}

#[test]
fn test_read_invalid_chunk() {
    let msg = b"4\r\nwik";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
}

#[test]
fn test_read_invalid_no_terminating_chunk() {
    let msg = b"4\r\nwiki";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::Other
    );
}

#[test]
fn test_read_invalid_bad_terminating_chunk() {
    let msg = b"4\r\nwiki\r\n0\r\n";
    let mut reader = ChunkedReader::new(BufReader::new(&msg[..]));
    let mut s = String::new();
    assert_eq!(
        reader.read_to_string(&mut s).err().unwrap().kind(),
        io::ErrorKind::Other
    );
}
