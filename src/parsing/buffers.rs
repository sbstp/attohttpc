use std::io::{self, BufRead, BufReader, Read};

pub fn read_line<R>(reader: &mut BufReader<R>, buf: &mut Vec<u8>) -> io::Result<usize>
where
    R: Read,
{
    buf.clear();
    let n = reader.read_until(b'\n', buf)?;

    if buf.ends_with(b"\r\n") {
        buf.truncate(buf.len() - 2);
    }

    if buf.ends_with(b"\n") {
        buf.truncate(buf.len() - 1);
    }

    Ok(n)
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
fn test_read_line_lf() {
    let mut reader = BufReader::new(&b"hello\nworld"[..]);
    let mut line = Vec::new();

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(6));
    assert_eq!(line, b"hello");

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(5));
    assert_eq!(line, b"world");
}

#[test]
fn test_read_line_crlf() {
    let mut reader = BufReader::new(&b"hello\r\nworld"[..]);
    let mut line = Vec::new();

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(7));
    assert_eq!(line, b"hello");

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(5));
    assert_eq!(line, b"world");
}

#[test]
fn test_read_line_empty() {
    let mut reader = BufReader::new(&b""[..]);
    let mut line = Vec::new();

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(0));
    assert_eq!(line, b"");
}

#[test]
fn test_read_line_empty_line() {
    let mut reader = BufReader::new(&b"\n"[..]);
    let mut line = Vec::new();

    assert_eq!(read_line(&mut reader, &mut line).ok(), Some(1));
    assert_eq!(line, b"");
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
