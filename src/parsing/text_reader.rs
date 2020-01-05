use std::fmt;
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
    internal_buf: Vec<u8>,
    internal_buf_amt: usize,
    eof: bool,
}

impl<R> TextReader<R>
where
    R: BufRead,
{
    /// Create a new `TextReader` with the given charset.
    pub fn new(inner: R, charset: Charset) -> TextReader<R> {
        TextReader {
            inner,
            decoder: charset.new_decoder(),
            internal_buf: vec![0u8; 4096],
            internal_buf_amt: 0,
            eof: false,
        }
    }
}

// impl<R> fmt::Debug for TextReader<R>
// where
//     R: fmt::Debug + BufRead,
// {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         f.debug_struct("TextReader")
//             .field("inner", &self.inner)
//             .field("decoder", &"<Decoder>")
//             .field("eof", &self.eof)
//             .finish()
//     }
// }

impl<R> Read for TextReader<R>
where
    R: BufRead,
{
    fn read(&mut self, mut dst: &mut [u8]) -> io::Result<usize> {
        // The `Read` trait is not directly compatible with the `encoding_rs` crate. There is no
        // way to signify to the caller of the `read` method that a larger buffer is required.
        // It is expected that as long as the destination buffer has a length larger than 0, some
        // data can be written into it. However, the `encoding_rs` crate will refuse to write
        // in a buffer that cannot contain a full code point. This is why we need to use a
        // middle-man buffer which always has enough room to be written into.
        //
        // Whenever the middle-man buffer contains data, it is first copied into the destination
        // buffer. When it's empty, we decode more data from the source buffer into it.

        if self.eof && self.internal_buf_amt == 0 {
            // If the inner reader is eof and the internal buffer is empty, we have nothing
            // else to do.
            return Ok(0);
        }

        let dst_len = dst.len();

        // Loop as long as the destination buffer has room and that the underlying stream is not
        // exhausted or that the internal buffer has data.
        while dst.len() > 0 && (!self.eof || self.internal_buf_amt > 0) {
            if self.internal_buf_amt > 0 {
                // Internal buffer contains some data. Copy it into dst.
                let n = std::cmp::min(dst.len(), self.internal_buf_amt);
                dst[..n].copy_from_slice(&self.internal_buf[..n]);
                self.internal_buf_amt -= n;
                dst = &mut dst[n..];
            } else {
                // Internal buffer is empty. Decode more data into it.
                let src = self.inner.fill_buf()?;
                if !src.is_empty() {
                    // Source buffer has data. Decode the data it contains into the internal buffer.
                    let (_, read, written, _) = self.decoder.decode_to_utf8(src, &mut self.internal_buf, false);
                    self.internal_buf_amt += written;
                    self.inner.consume(read);
                } else {
                    // EOF has been reached in the underlying stream. Source buffer is empty.
                    // We must finalize the decoding.
                    let (res, _, written, _) = self.decoder.decode_to_utf8(src, &mut self.internal_buf, true);
                    self.internal_buf_amt += written;

                    // If the finalization was successful, we can set eof.
                    if res == CoderResult::InputEmpty {
                        self.eof = true;
                    }
                }
            }
        }

        dbg!(Ok(dst_len - dst.len()))
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
    assert_eq!(20_000, reader.read_to_string(&mut text).unwrap());

    assert_eq!(text.len(), 20_000);

    for c in text.chars() {
        assert_eq!(c, 'É');
    }
}
