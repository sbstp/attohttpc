use std::cmp;
use std::io::{self, Read};

pub struct LengthReader<R>
where
    R: Read,
{
    inner: R,
    read: u64,
    length: u64,
}

impl<R> LengthReader<R>
where
    R: Read,
{
    pub fn new(inner: R, length: u64) -> LengthReader<R> {
        LengthReader { inner, length, read: 0 }
    }
}

impl<R> Read for LengthReader<R>
where
    R: Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.length - self.read;
        debug!("remaining={} buflen={}", remaining, buf.len());
        if remaining == 0 {
            return Ok(0);
        }
        let count = cmp::min(buf.len() as u64, remaining) as usize;
        let n = self.inner.read(&mut buf[..count])?;
        self.read += n as u64;
        debug!("read {} bytes", n);
        Ok(n)
    }
}

#[test]
fn test_read_works() {
    let mut reader = LengthReader::new(&b"hello"[..], 5);
    let mut buf = [0u8; 1024];

    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..n], b"hello");

    // test eof
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn test_reads_no_more() {
    let mut reader = LengthReader::new(&b"hello world"[..], 5);
    let mut buf = [0u8; 1024];

    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 5);
    assert_eq!(&buf[..n], b"hello");

    // test eof
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn test_read_tiny_buf() {
    let mut reader = LengthReader::new(&b"hello"[..], 5);
    let mut buf = [0u8; 2];

    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 2);
    assert_eq!(&buf[..n], b"he");

    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 2);
    assert_eq!(&buf[..n], b"ll");

    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 1);
    assert_eq!(&buf[..n], b"o");

    // test eof
    let n = reader.read(&mut buf).unwrap();
    assert_eq!(n, 0);
}
