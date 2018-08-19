use std::io::{self, Read, Write};
use std::net::TcpStream;

use native_tls::{HandshakeError, TlsConnector, TlsStream};

use crate::error::HttpResult;

pub enum MaybeTls {
    Normal(TcpStream),
    Tls(TlsStream<TcpStream>),
}

impl MaybeTls {
    pub fn connect(host: &str, port: u16) -> HttpResult<MaybeTls> {
        let stream = TcpStream::connect((host, port))?;
        Ok(MaybeTls::Normal(stream))
    }

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
}

impl Read for MaybeTls {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            MaybeTls::Normal(s) => s.read(buf),
            MaybeTls::Tls(s) => s.read(buf),
        }
    }
}

impl Write for MaybeTls {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            MaybeTls::Normal(s) => s.write(buf),
            MaybeTls::Tls(s) => s.write(buf),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            MaybeTls::Normal(s) => s.flush(),
            MaybeTls::Tls(s) => s.flush(),
        }
    }
}
