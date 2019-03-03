#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::TcpStream;

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};

use crate::error::HttpResult;

pub enum MaybeTls {
    Normal(TcpStream),
    #[cfg(feature = "tls")]
    Tls(TlsStream<TcpStream>),
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl MaybeTls {
    pub fn connect(host: &str, port: u16) -> HttpResult<MaybeTls> {
        let stream = TcpStream::connect((host, port))?;
        Ok(MaybeTls::Normal(stream))
    }

    #[cfg(feature = "tls")]
    pub fn connect_tls(host: &str, port: u16) -> HttpResult<MaybeTls> {
        let connector = TlsConnector::new()?;
        let stream = TcpStream::connect((host, port))?;
        let tls_stream = match connector.connect(host, stream) {
            Ok(stream) => stream,
            Err(HandshakeError::Failure(err)) => return Err(err.into()),
            Err(HandshakeError::WouldBlock(_)) => panic!("socket configured in non-blocking mode"),
        };
        Ok(MaybeTls::Tls(tls_stream))
    }

    #[cfg(test)]
    pub fn mock(bytes: Vec<u8>) -> MaybeTls {
        MaybeTls::Mock(Cursor::new(bytes))
    }
}

impl Read for MaybeTls {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            MaybeTls::Normal(s) => s.read(buf),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(s) => s.read(buf),
            #[cfg(test)]
            MaybeTls::Mock(s) => s.read(buf),
        }
    }
}

impl Write for MaybeTls {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            MaybeTls::Normal(s) => s.write(buf),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(s) => s.write(buf),
            #[cfg(test)]
            _ => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            MaybeTls::Normal(s) => s.flush(),
            #[cfg(feature = "tls")]
            MaybeTls::Tls(s) => s.flush(),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}
