#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::thread;

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
use url::Url;

use crate::happy;
use crate::request::BaseSettings;
use crate::{ErrorKind, Result};

pub struct ConnectInfo<'a> {
    pub url: &'a Url,
    pub base_settings: &'a BaseSettings,
}

#[derive(Debug)]
pub enum BaseStream {
    Plain {
        stream: TcpStream,
        timeout: Option<mpsc::Sender<()>>,
    },
    #[cfg(feature = "tls")]
    Tls {
        stream: TlsStream<TcpStream>,
        timeout: Option<mpsc::Sender<()>>,
    },
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(info: &ConnectInfo) -> Result<BaseStream> {
        let host = info.url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = info.url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        match info.url.scheme() {
            "http" => {
                BaseStream::connect_tcp(host, port, info).map(|(stream, timeout)| BaseStream::Plain { stream, timeout })
            }
            #[cfg(feature = "tls")]
            "https" => {
                BaseStream::connect_tls(host, port, info).map(|(stream, timeout)| BaseStream::Tls { stream, timeout })
            }
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }
    }

    fn connect_tcp(host: &str, port: u16, info: &ConnectInfo) -> Result<(TcpStream, Option<mpsc::Sender<()>>)> {
        let stream = happy::connect((host, port), info.base_settings.connect_timeout)?;
        stream.set_read_timeout(Some(info.base_settings.read_timeout))?;
        let timeout = info
            .base_settings
            .timeout
            .map(|timeout| -> Result<mpsc::Sender<()>> {
                let stream = stream.try_clone()?;
                let (tx, rx) = mpsc::channel();
                thread::spawn(move || {
                    if let Err(mpsc::RecvTimeoutError::Timeout) = rx.recv_timeout(timeout) {
                        drop(rx);
                        let _ = stream.shutdown(Shutdown::Both);
                    }
                });
                Ok(tx)
            })
            .transpose()?;
        Ok((stream, timeout))
    }

    #[cfg(feature = "tls")]
    fn connect_tls(
        host: &str,
        port: u16,
        info: &ConnectInfo,
    ) -> Result<(TlsStream<TcpStream>, Option<mpsc::Sender<()>>)> {
        let connector = TlsConnector::builder()
            .danger_accept_invalid_certs(info.base_settings.accept_invalid_certs)
            .danger_accept_invalid_hostnames(info.base_settings.accept_invalid_hostnames)
            .build()?;
        let (stream, timeout) = BaseStream::connect_tcp(host, port, info)?;
        let stream = match connector.connect(host, stream) {
            Ok(stream) => stream,
            Err(HandshakeError::Failure(err)) => return Err(err.into()),
            Err(HandshakeError::WouldBlock(mut stream)) => loop {
                match stream.handshake() {
                    Ok(stream) => break stream,
                    Err(HandshakeError::Failure(err)) => return Err(err.into()),
                    Err(HandshakeError::WouldBlock(mid_stream)) => stream = mid_stream,
                }
            },
        };
        Ok((stream, timeout))
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
            BaseStream::Plain { stream, timeout } => read_timeout(stream, buf, timeout),
            #[cfg(feature = "tls")]
            BaseStream::Tls { stream, timeout } => read_timeout(stream, buf, timeout),
            #[cfg(test)]
            BaseStream::Mock(s) => s.read(buf),
        }
    }
}

impl Write for BaseStream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            BaseStream::Plain { stream, .. } => stream.write(buf),
            #[cfg(feature = "tls")]
            BaseStream::Tls { stream, .. } => stream.write(buf),
            #[cfg(test)]
            _ => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            BaseStream::Plain { stream, .. } => stream.flush(),
            #[cfg(feature = "tls")]
            BaseStream::Tls { stream, .. } => stream.flush(),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}

fn read_timeout(mut stream: impl Read, buf: &mut [u8], timeout: &Option<mpsc::Sender<()>>) -> io::Result<usize> {
    let read = stream.read(buf)?;

    if let Some(timeout) = timeout {
        if read == 0 && !buf.is_empty() && timeout.send(()).is_err() {
            return Err(io::ErrorKind::TimedOut.into());
        }
    }

    Ok(read)
}
