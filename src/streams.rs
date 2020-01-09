#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
use url::Url;

use crate::happy;
use crate::{ErrorKind, Result};

pub struct ConnectInfo<'u> {
    pub url: &'u Url,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    #[cfg(feature = "tls")]
    pub accept_invalid_certs: bool,
    #[cfg(feature = "tls")]
    pub accept_invalid_hostnames: bool,
}

#[derive(Debug)]
pub enum BaseStream {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(TlsStream<TcpStream>),
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(info: &ConnectInfo) -> Result<BaseStream> {
        let host = info.url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = info.url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        match info.url.scheme() {
            "http" => BaseStream::connect_tcp(host, port, info).map(BaseStream::Plain),
            #[cfg(feature = "tls")]
            "https" => BaseStream::connect_tls(host, port, info).map(BaseStream::Tls),
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }
    }

    fn connect_tcp(host: &str, port: u16, info: &ConnectInfo) -> Result<TcpStream> {
        let stream = happy::connect((host, port), info.connect_timeout)?;
        stream.set_read_timeout(Some(info.read_timeout))?;
        Ok(stream)
    }

    #[cfg(feature = "tls")]
    fn connect_tls(host: &str, port: u16, info: &ConnectInfo) -> Result<TlsStream<TcpStream>> {
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(info.accept_invalid_certs)
            .danger_accept_invalid_hostnames(info.accept_invalid_hostnames)
            .build()?;
        let stream = BaseStream::connect_tcp(host, port, info)?;
        let tls_stream = match connector.connect(host, stream) {
            Ok(stream) => stream,
            Err(HandshakeError::Failure(err)) => return Err(err.into()),
            Err(HandshakeError::WouldBlock(_)) => panic!("socket configured in non-blocking mode"),
        };
        Ok(tls_stream)
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
