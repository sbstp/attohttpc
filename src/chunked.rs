use std::io::{self, Read};
use std::ptr;

pub const BUFFER_SIZE: usize = 8192;

pub struct BufReader<R>
where
    R: Read,
{
    inner: R,
    buff: Vec<u8>,
    start: usize,
    pos: usize,
}

impl<R> BufReader<R>
where
    R: Read,
{
    pub fn new(reader: R) -> BufReader<R> {
        BufReader {
            inner: reader,
            buff: Vec::new(),
            start: 0,
            pos: 0,
        }
    }

    fn fill(&mut self) -> io::Result<()> {
        assert!(self.pos == self.buff.len());

        if self.start > 0 {
            // If the start index is not zero, we move the contents to the front of
            // the buffer in order to overwrite the consumed data. The remainder
            // data is anything after the start index.
            let rem = self.buff.len() - self.start;
            unsafe {
                ptr::copy(self.buff[self.start..].as_ptr(), self.buff[0..].as_mut_ptr(), rem);
                self.buff.set_len(rem);
            }
            // After the copy, the amount of overwritten bytes is removed from the position.
            // Start is reset to 0.
            self.pos = self.pos - self.start;
            self.start = 0;
        }

        if self.pos >= self.buff.capacity() {
            // If the position and the buffer's capacity are the same,
            // the buffer is full. We must grow it.
            self.buff.reserve(BUFFER_SIZE);
        }

        // Fill the buffer up to its capacity or less.
        unsafe {
            let len = self.buff.len();
            self.buff.set_len(self.buff.capacity());
            let n = self.inner.read(&mut self.buff[self.pos..])?;
            self.buff.set_len(len + n);
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "eof"));
            }
        }

        Ok(())
    }

    /// Get the next byte from the stream.
    ///
    /// If the stream is at its end, an error is returned.
    /// If the buffer has no more data, more is fetched and the
    /// internal buffer can possibly be grown.
    pub fn next(&mut self) -> io::Result<u8> {
        if self.pos >= self.buff.len() {
            self.fill()?;
        }
        let b = self.buff[self.pos];
        self.pos += 1;
        Ok(b)
    }

    /// Advances the cursor to the reading position.
    pub fn consume(&mut self) {
        self.start = self.pos
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
                return Ok(&self.buff[self.start..self.pos - 1]);
            }
            if next == b'\r' && self.next()? == b'\n' {
                return Ok(&self.buff[self.start..self.pos - 2]);
            }
        }
    }
}

#[test]
fn test_fill_grow() {
    let mut reader = BufReader::new(&b"hello"[..]);
    assert_eq!(reader.next().unwrap(), b'h');
    assert_eq!(reader.buff.capacity(), BUFFER_SIZE);
    assert_eq!(reader.buff.len(), 5);
    assert_eq!(reader.start, 0);
    assert_eq!(reader.pos, 1);
}

#[test]
fn test_fill_copy() {
    let mut buff = Vec::with_capacity(BUFFER_SIZE);
    buff.push(b'h');

    assert_eq!(buff.len(), 1);
    assert_eq!(buff.capacity(), BUFFER_SIZE);

    let mut reader = BufReader {
        inner: &b"ello"[..],
        buff: buff,
        start: 1,
        pos: 1,
    };

    assert_eq!(reader.next().unwrap(), b'e');
    assert_eq!(reader.buff.len(), 4);
    assert_eq!(reader.buff, b"ello");
    assert_eq!(reader.buff.capacity(), BUFFER_SIZE);
}

#[test]
fn test_fill_grow_copy() {
    let mut buff = Vec::with_capacity(1);
    buff.push(b'h');

    assert_eq!(buff.len(), 1);
    assert_eq!(buff.capacity(), 1);

    let mut reader = BufReader {
        inner: &b"ello"[..],
        buff: buff,
        start: 1,
        pos: 1,
    };

    assert_eq!(reader.next().unwrap(), b'e'); // capacity is 1, start is 1 so no growth is needed
    assert_eq!(reader.next().unwrap(), b'l'); // capacity is 1, start is 0 and there's 1 item in buff, growth needed
    assert_eq!(reader.buff.len(), 4);
    assert_eq!(reader.buff, b"ello");
    assert_eq!(reader.buff.capacity(), BUFFER_SIZE + 1);
}

#[test]
fn test_consume() {
    let mut reader = BufReader::new(&b"hello"[..]);
    assert_eq!(reader.next().unwrap(), b'h');
    assert_eq!(reader.next().unwrap(), b'e');
    assert_eq!(reader.start, 0);
    assert_eq!(reader.pos, 2);
    reader.consume();
    assert_eq!(reader.start, 2);
    assert_eq!(reader.pos, 2);
}

#[test]
fn test_read_line_lf() {
    let mut reader = BufReader::new(&b"hello\n"[..]);
    assert_eq!(reader.read_line().unwrap(), b"hello");
    assert_eq!(reader.pos, 6);
}

#[test]
fn test_read_line_crlf() {
    let mut reader = BufReader::new(&b"hello\r\n"[..]);
    assert_eq!(reader.read_line().unwrap(), b"hello");
    assert_eq!(reader.pos, 7);
}

#[test]
fn test_read_line_eof() {
    let mut reader = BufReader::new(&b"hello"[..]);
    assert!(reader.read_line().is_err());
}
