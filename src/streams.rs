#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::TcpStream;

#[cfg(feature = "charsets")]
use encoding_rs::{self, CoderResult};
#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
use url::Url;

#[cfg(feature = "charsets")]
use crate::charsets::Charset;
use crate::{HttpError, HttpResult};

pub enum BaseStream {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(TlsStream<TcpStream>),
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(url: &Url) -> HttpResult<BaseStream> {
        let host = url.host_str().ok_or(HttpError::InvalidUrl("url has no host"))?;
        let port = url
            .port_or_known_default()
            .ok_or(HttpError::InvalidUrl("url has no port"))?;

        debug!("trying to connect to {}:{}", host, port);

        Ok(match url.scheme() {
            "http" => BaseStream::Plain(TcpStream::connect((host, port))?),
            #[cfg(feature = "tls")]
            "https" => BaseStream::connect_tls(host, port)?,
            _ => return Err(HttpError::InvalidUrl("url contains unsupported scheme")),
        })
    }

    #[cfg(feature = "tls")]
    fn connect_tls(host: &str, port: u16) -> HttpResult<BaseStream> {
        let connector = TlsConnector::new()?;
        let stream = TcpStream::connect((host, port))?;
        let tls_stream = match connector.connect(host, stream) {
            Ok(stream) => stream,
            Err(HandshakeError::Failure(err)) => return Err(err.into()),
            Err(HandshakeError::WouldBlock(_)) => panic!("socket configured in non-blocking mode"),
        };
        Ok(BaseStream::Tls(tls_stream))
    }

    #[cfg(test)]
    pub fn mock(bytes: Vec<u8>) -> BaseStream {
        BaseStream::Mock(Cursor::new(bytes))
    }
}

impl Read for BaseStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            BaseStream::Plain(s) => s.read(buf),
            #[cfg(feature = "tls")]
            BaseStream::Tls(s) => s.read(buf),
            #[cfg(test)]
            BaseStream::Mock(s) => s.read(buf),
        }
    }
}

impl Write for BaseStream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            BaseStream::Plain(s) => s.write(buf),
            #[cfg(feature = "tls")]
            BaseStream::Tls(s) => s.write(buf),
            #[cfg(test)]
            _ => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            BaseStream::Plain(s) => s.flush(),
            #[cfg(feature = "tls")]
            BaseStream::Tls(s) => s.flush(),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}

#[cfg(feature = "charsets")]
pub struct StreamDecoder {
    output: String,
    decoder: encoding_rs::Decoder,
}

#[cfg(feature = "charsets")]
impl StreamDecoder {
    pub fn new(charset: Charset) -> StreamDecoder {
        StreamDecoder {
            output: String::with_capacity(1024),
            decoder: charset.new_decoder(),
        }
    }

    pub fn take(mut self) -> String {
        self.decoder.decode_to_string(&[], &mut self.output, true);
        self.output
    }
}

#[cfg(feature = "charsets")]
impl Write for StreamDecoder {
    fn write(&mut self, mut buf: &[u8]) -> io::Result<usize> {
        let len = buf.len();
        while buf.len() > 0 {
            match self.decoder.decode_to_string(&buf, &mut self.output, false) {
                (CoderResult::InputEmpty, written, _) => {
                    buf = &buf[written..];
                }
                (CoderResult::OutputFull, written, _) => {
                    buf = &buf[written..];
                    self.output.reserve(self.output.capacity());
                }
            }
        }
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::StreamDecoder;
    use crate::charsets;
    use std::io::Write;

    #[test]
    fn test_stream_decoder_utf8() {
        let mut decoder = StreamDecoder::new(charsets::UTF_8);
        decoder.write_all("québec".as_bytes()).unwrap();
        assert_eq!(decoder.take(), "québec");
    }

    #[test]
    fn test_stream_decoder_latin1() {
        let mut decoder = StreamDecoder::new(charsets::WINDOWS_1252);
        decoder.write_all(&[201]).unwrap();
        assert_eq!(decoder.take(), "É");
    }

    #[test]
    fn test_stream_decoder_large_buffer() {
        let mut decoder = StreamDecoder::new(charsets::WINDOWS_1252);
        let mut buf = vec![];
        for _ in 0..10_000 {
            buf.push(201);
        }
        decoder.write_all(&buf).unwrap();
        for c in decoder.take().chars() {
            assert_eq!(c, 'É');
        }
    }
}
