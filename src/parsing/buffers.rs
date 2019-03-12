use std::cmp;
use std::io::{self, Read};
use std::ptr;

pub const DEFAULT_CHUNK_SIZE: usize = 8192;

pub struct ExpandingBufReader<R>
where
    R: Read,
{
    inner: R,
    buff: Vec<u8>,     // internal buffer
    pos: usize,        // position of the next byte read
    mark: usize,       // position of the mark
    chunk_size: usize, // size of the buffer's allocation chunks
}

impl<R> ExpandingBufReader<R>
where
    R: Read,
{
    pub fn new(reader: R) -> ExpandingBufReader<R> {
        ExpandingBufReader::with_chunk_size(reader, DEFAULT_CHUNK_SIZE)
    }

    pub fn with_chunk_size(reader: R, chunk_size: usize) -> ExpandingBufReader<R> {
        ExpandingBufReader {
            inner: reader,
            buff: Vec::new(),
            mark: 0,
            pos: 0,
            chunk_size,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        debug_assert!(self.pos == self.buff.len());

        if self.mark > 0 {
            // Any data before the mark position can be overwritten. Before growing the internal
            // buffer, we try to move all the protected data to the front of the buffer, overwriting
            // everything before the last slice.
            let protected = self.pos - self.mark;

            if protected == 0 {
                // If there are no protected bytes in the buffer, we can simply clear it.
                self.buff.clear();
                self.mark = 0;
                self.pos = 0;
            } else {
                // If there are protected bytes left in the buffer we must move them to the front.
                unsafe {
                    ptr::copy(self.buff[self.mark..].as_ptr(), self.buff[0..].as_mut_ptr(), protected);
                    self.buff.set_len(protected);
                }
                // The new reading position starts right after the number of protected bytes.
                self.pos = protected;
                self.mark = 0;
            }
        }

        if self.pos >= self.buff.capacity() {
            // If the read position and the buffer's capacity are the same,
            // the buffer is full. We must grow it.
            self.buff.reserve(self.chunk_size);
        }

        // Fill the buffer up to its capacity.
        unsafe {
            let len = self.buff.len();
            self.buff.set_len(self.buff.capacity());
            let n = self.inner.read(&mut self.buff[self.pos..])?;
            self.buff.set_len(len + n);
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
        }

        Ok(())
    }

    /// Set the mark to be the position.
    fn advance(&mut self) {
        self.mark = self.pos;
    }

    /// Get the next byte from the stream.
    ///
    /// If the stream is at its end, an error is returned.
    /// If the buffer has no more data, more is fetched and the
    /// internal buffer can possibly be grown.
    fn next(&mut self) -> io::Result<u8> {
        if self.pos >= self.buff.len() {
            self.fill()?;
        }
        let b = self.buff[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Access the data from the start cursor to the read cursor skipping `skip` bytes at the end.
    #[inline]
    fn slice_off(&mut self, skip: usize) -> &[u8] {
        let start = self.mark;
        self.advance();
        &self.buff[start..self.pos - skip]
    }

    /// Read a line from the reader, until `\n` or `\r\n`.
    ///
    /// Advances the position until line feed characters are found.
    /// Consumes the line feed characters. The returned slice is
    /// chomped and does not contain the line feed characters.
    pub fn read_line(&mut self) -> io::Result<&[u8]> {
        loop {
            let next = self.next()?;
            if next == b'\n' {
                return Ok(self.slice_off(1));
            }
            if next == b'\r' && self.next()? == b'\n' {
                return Ok(self.slice_off(2));
            }
        }
    }
}

impl<R> Read for ExpandingBufReader<R>
where
    R: Read,
{
    /// Read some data from this reader.
    ///
    /// If any buffered data remains, it is copied to the slice.
    /// If no data remains in the internal buffer, data is read
    /// from the underlying stream.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.buff.len() - self.mark;
        if remaining > 0 {
            let amount = cmp::min(remaining, buf.len());
            unsafe {
                ptr::copy_nonoverlapping(self.buff[self.mark..].as_ptr(), buf.as_mut_ptr(), amount);
            }
            self.mark = self.mark + amount;
            self.pos = self.mark;
            Ok(amount)
        } else {
            let n = self.inner.read(buf)?;
            // Void the internal buffer's data since we bypassed it.
            self.pos = self.buff.len();
            self.mark = self.buff.len();
            Ok(n)
        }
    }
}

pub fn trim_byte(byte: u8, buf: &[u8]) -> &[u8] {
    trim_byte_left(byte, trim_byte_right(byte, buf))
}

pub fn trim_byte_left(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.first().map(|&b| b) {
        if b == byte {
            unsafe {
                buf = &buf.get_unchecked(1..);
            }
        } else {
            break;
        }
    }
    buf
}

pub fn trim_byte_right(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.last().map(|&b| b) {
        if b == byte {
            unsafe {
                buf = &buf.get_unchecked(..buf.len() - 1);
            }
        } else {
            break;
        }
    }
    buf
}

#[test]
fn test_fill_grow() {
    let mut reader = ExpandingBufReader::new(&b"hello"[..]);
    assert_eq!(reader.next().unwrap(), b'h');
    assert_eq!(reader.buff.capacity(), DEFAULT_CHUNK_SIZE);
    assert_eq!(reader.buff.len(), 5);
    assert_eq!(reader.mark, 0);
    assert_eq!(reader.pos, 1);
}

#[test]
fn test_fill_grow_copy() {
    let mut reader = ExpandingBufReader::with_chunk_size(&b"hello"[..], 1);
    assert_eq!(reader.next().unwrap(), b'h');
    reader.advance();
    assert_eq!(reader.mark, 1);

    // read that overwrites consumed data
    assert_eq!(reader.next().unwrap(), b'e');
    assert_eq!(reader.mark, 0);
    assert_eq!(reader.pos, 1);
    assert_eq!(reader.buff.len(), 1);

    // read that grows the buffer
    assert_eq!(reader.next().unwrap(), b'l');
    assert_eq!(reader.mark, 0);
    assert_eq!(reader.pos, 2);
    assert_eq!(reader.buff.len(), 2);
}

#[test]
fn test_advance() {
    let mut reader = ExpandingBufReader::new(&b"hello"[..]);
    assert_eq!(reader.next().unwrap(), b'h');
    assert_eq!(reader.next().unwrap(), b'e');
    assert_eq!(reader.mark, 0);
    assert_eq!(reader.pos, 2);

    reader.advance();

    assert_eq!(reader.mark, 2);
    assert_eq!(reader.pos, 2);
}

#[test]
fn test_read_line_lf() {
    let mut reader = ExpandingBufReader::new(&b"hello\n"[..]);
    assert_eq!(reader.read_line().unwrap(), b"hello");
    assert_eq!(reader.pos, 6);
}

#[test]
fn test_read_line_crlf() {
    let mut reader = ExpandingBufReader::new(&b"hello\r\n"[..]);
    assert_eq!(reader.read_line().unwrap(), b"hello");
    assert_eq!(reader.pos, 7);
}

#[test]
fn test_read_line_eof() {
    let mut reader = ExpandingBufReader::new(&b"hello"[..]);
    assert!(reader.read_line().is_err());
}

#[test]
fn test_read_line_small_chunks() {
    let mut reader = ExpandingBufReader::with_chunk_size(&b"hello world!\r\n"[..], 2);
    let line = reader.read_line().unwrap();
    assert_eq!(line, b"hello world!");
    assert_eq!(reader.buff.len(), 14);
    assert_eq!(reader.pos, 14);
    assert_eq!(reader.mark, 14);
}

#[test]
fn test_read() {
    let mut reader = ExpandingBufReader::with_chunk_size(&b"hello"[..], 2);
    reader.next().unwrap();
    reader.advance();

    let mut buf = [0u8; 1024];

    // read the rest of the buffered stuff
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(&buf[..n], b"e");

    // read 2 bytes using bypasse
    let n = reader.read(&mut buf[..2]).unwrap();
    assert_eq!(n, 2);
    assert_eq!(&buf[..n], b"ll");

    // next read using the buffer should clear everything
    assert_eq!(reader.next().unwrap(), b'o');
    assert_eq!(reader.buff, b"o");
}

#[test]
fn test_slice_off() {
    let mut reader = ExpandingBufReader::new(&b"hello"[..]);
    reader.next().unwrap();
    reader.next().unwrap();
    reader.next().unwrap();
    assert_eq!(reader.slice_off(1), b"he");
}

#[test]
fn test_trim_byte() {
    assert_eq!(trim_byte(b' ', b"  hello  "), b"hello");
    assert_eq!(trim_byte(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte(b' ', b""), b"");
}

#[test]
fn test_trim_byte_left() {
    assert_eq!(trim_byte_left(b' ', b"  hello"), b"hello");
    assert_eq!(trim_byte_left(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte_left(b' ', b""), b"");
}

#[test]
fn test_trim_byte_right() {
    assert_eq!(trim_byte_right(b' ', b"hello  "), b"hello");
    assert_eq!(trim_byte_right(b' ', b"hello"), b"hello");
    assert_eq!(trim_byte_right(b' ', b""), b"");
}
