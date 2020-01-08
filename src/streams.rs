#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
use url::Url;

use crate::happy;
use crate::{ErrorKind, Result};

#[derive(Debug)]
pub struct BaseStream {
    kind: StreamKind,
    deadline: Option<Instant>,
}

#[derive(Debug)]
enum StreamKind {
    Plain(TcpStream),
    #[cfg(feature = "tls")]
    Tls(TlsStream<TcpStream>),
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(url: &Url, deadline: Option<Instant>) -> Result<Self> {
        let host = url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        match url.scheme() {
            "http" => {
                let stream = happy::connect((host, port), None)?;

                Ok(Self {
                    kind: StreamKind::Plain(stream),
                    deadline,
                })
            }
            #[cfg(feature = "tls")]
            "https" => Self::connect_tls(host, port, deadline),
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }
    }

    #[cfg(feature = "tls")]
    fn connect_tls(host: &str, port: u16, deadline: Option<Instant>) -> Result<Self> {
        let connector = TlsConnector::new()?;
        let stream = happy::connect((host, port), None)?;
        let tls_stream = match connector.connect(host, stream) {
            Ok(stream) => stream,
            Err(HandshakeError::Failure(err)) => return Err(err.into()),
            Err(HandshakeError::WouldBlock(_)) => panic!("socket configured in non-blocking mode"),
        };
        Ok(Self {
            kind: StreamKind::Tls(tls_stream),
            deadline,
        })
    }

    #[cfg(test)]
    pub fn mock(bytes: Vec<u8>) -> Self {
        Self {
            kind: StreamKind::Mock(Cursor::new(bytes)),
            deadline: None,
        }
    }
}

impl Read for BaseStream {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.kind {
            StreamKind::Plain(s) => {
                apply_read_deadline(self.deadline, s)?;
                s.read(buf)
            }
            #[cfg(feature = "tls")]
            StreamKind::Tls(s) => {
                apply_read_deadline(self.deadline, s.get_mut())?;
                s.read(buf)
            }
            #[cfg(test)]
            StreamKind::Mock(s) => s.read(buf),
        }
    }
}

impl Write for BaseStream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.kind {
            StreamKind::Plain(s) => {
                apply_write_deadline(self.deadline, s)?;
                s.write(buf)
            }
            #[cfg(feature = "tls")]
            StreamKind::Tls(s) => {
                apply_write_deadline(self.deadline, s.get_mut())?;
                s.write(buf)
            }
            #[cfg(test)]
            _ => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match &mut self.kind {
            StreamKind::Plain(s) => s.flush(),
            #[cfg(feature = "tls")]
            StreamKind::Tls(s) => s.flush(),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}

fn apply_read_deadline(deadline: Option<Instant>, stream: &mut TcpStream) -> io::Result<()> {
    apply_deadline(deadline, |timeout| stream.set_read_timeout(Some(timeout)))
}

fn apply_write_deadline(deadline: Option<Instant>, stream: &mut TcpStream) -> io::Result<()> {
    apply_deadline(deadline, |timeout| stream.set_write_timeout(Some(timeout)))
}

fn apply_deadline(deadline: Option<Instant>, set_timeout: impl FnOnce(Duration) -> io::Result<()>) -> io::Result<()> {
    if let Some(deadline) = deadline {
        let now = Instant::now();

        if deadline <= now {
            return Err(io::ErrorKind::TimedOut.into());
        }

        set_timeout(deadline - now)?;
    }

    Ok(())
}
