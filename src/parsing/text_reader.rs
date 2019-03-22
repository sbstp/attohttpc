use std::io::{self, BufRead, Read};

use encoding_rs::{CoderResult, Decoder};

use crate::charsets::Charset;

/// `TextReader` converts bytes in a specific charset to bytes in UTF-8.
///
/// It can be used to convert a stream of text in a specific charset into a stream
/// of UTF-8 encoded bytes. The `Read::read_to_string` method can be used to convert
/// the stream of UTF-8 bytes into a `String`.
pub struct TextReader<R>
where
    R: BufRead,
{
    inner: R,
    decoder: Decoder,
    eof: bool,
}

impl<R> TextReader<R>
where
    R: BufRead,
{
    /// Create a new `TextReader` with the given charset.
    pub fn new(inner: R, charset: Charset) -> TextReader<R> {
        TextReader {
            inner: inner,
            decoder: charset.new_decoder(),
            eof: false,
        }
    }
}

impl<R> Read for TextReader<R>
where
    R: BufRead,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.eof {
            return Ok(0);
        }

        let mut total_written = 0;

        loop {
            let src = self.inner.fill_buf()?;
            if src.is_empty() {
                // inner has reached EOF, write last to the buffer.
                let (res, _, written, _) = self.decoder.decode_to_utf8(src, buf, true);
                total_written += written;

                match res {
                    CoderResult::InputEmpty => {
                        // last call was successful, set eof to true
                        self.eof = true;
                        break;
                    }
                    CoderResult::OutputFull => {
                        // last call was not successful because the output is full, try again in the next call to `read`
                        break;
                    }
                }
            } else {
                let (res, read, written, _) = self.decoder.decode_to_utf8(src, buf, false);

                self.inner.consume(read);
                total_written += written;

                match res {
                    CoderResult::InputEmpty => {
                        // read all the bytes available in src, read more
                        continue;
                    }
                    CoderResult::OutputFull => {
                        // buf is full, break and return the number read
                        break;
                    }
                }
            }
        }

        Ok(total_written)
    }
}

#[test]
fn test_stream_decoder_utf8() {
    let mut reader = TextReader::new("québec".as_bytes(), crate::charsets::UTF_8);

    let mut text = String::new();
    assert_eq!(reader.read_to_string(&mut text).ok(), Some(7));

    assert_eq!(text, "québec");
}

#[test]
fn test_stream_decoder_latin1() {
    let mut reader = TextReader::new(&b"qu\xC9bec"[..], crate::charsets::WINDOWS_1252);

    let mut text = String::new();
    assert_eq!(reader.read_to_string(&mut text).ok(), Some(7));

    assert_eq!(text, "quÉbec");
}

#[test]
fn test_string_reader_large_buffer_latin1() {
    let mut buf = vec![];
    for _ in 0..10_000 {
        buf.push(201);
    }
    let mut reader = TextReader::new(&buf[..], crate::charsets::WINDOWS_1252);

    let mut text = String::new();
    reader.read_to_string(&mut text).unwrap();

    for c in text.chars() {
        assert_eq!(c, 'É');
    }
}
