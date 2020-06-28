#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
#[cfg(feature = "tls-rustls")]
use std::sync::Arc;
use std::thread;

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
#[cfg(feature = "tls-rustls")]
use rustls::{ClientConfig, ClientSession, Session, StreamOwned};
use url::{Host, Url};
#[cfg(feature = "tls-rustls")]
use webpki::DNSNameRef;
#[cfg(feature = "tls-rustls")]
use webpki_roots::TLS_SERVER_ROOTS;

use crate::happy;
use crate::request::BaseSettings;
#[cfg(feature = "tls-rustls")]
use crate::skip_debug::SkipDebug;
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
    #[cfg(feature = "tls-rustls")]
    Rustls {
        stream: SkipDebug<Box<StreamOwned<ClientSession, TcpStream>>>,
        timeout: Option<mpsc::Sender<()>>,
    },
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(info: &ConnectInfo) -> Result<BaseStream> {
        let host = info.url.host().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = info.url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        #[allow(unreachable_patterns)]
        match info.url.scheme() {
            "http" => {
                BaseStream::connect_tcp(host, port, info).map(|(stream, timeout)| BaseStream::Plain { stream, timeout })
            }
            #[cfg(feature = "tls")]
            "https" => BaseStream::connect_tls(host, port, info),
            #[cfg(feature = "tls-rustls")]
            "https" => BaseStream::connect_rustls(host, port, info),
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }
    }

    fn connect_tcp(host: Host<&str>, port: u16, info: &ConnectInfo) -> Result<(TcpStream, Option<mpsc::Sender<()>>)> {
        let stream = happy::connect(host, port, info.base_settings.connect_timeout)?;
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
    fn connect_tls(host: Host<&str>, port: u16, info: &ConnectInfo) -> Result<BaseStream> {
        let mut connector_builder = TlsConnector::builder();
        connector_builder.danger_accept_invalid_certs(info.base_settings.accept_invalid_certs);
        connector_builder.danger_accept_invalid_hostnames(info.base_settings.accept_invalid_hostnames);
        for cert in &info.base_settings.root_certificates.0 {
            connector_builder.add_root_certificate(cert.clone());
        }
        let connector = connector_builder.build()?;
        let (stream, timeout) = BaseStream::connect_tcp(host, port, info)?;
        let host_str = info.url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
        let stream = match connector.connect(host_str, stream) {
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
        Ok(BaseStream::Tls { stream, timeout })
    }

    #[cfg(feature = "tls-rustls")]
    fn connect_rustls(host: Host<&str>, port: u16, info: &ConnectInfo) -> Result<BaseStream> {
        let host_str = info.url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
        let name = DNSNameRef::try_from_ascii_str(host_str)?;
        let (mut stream, timeout) = BaseStream::connect_tcp(host, port, info)?;

        let mut session = match &info.base_settings.client_config.0 {
            Some(client_config) => ClientSession::new(client_config, name),
            None => {
                let mut client_config = ClientConfig::new();
                client_config.root_store.add_server_trust_anchors(&TLS_SERVER_ROOTS);
                ClientSession::new(&Arc::new(client_config), name)
            }
        };

        while let Err(err) = session.complete_io(&mut stream) {
            if err.kind() != io::ErrorKind::WouldBlock || !session.is_handshaking() {
                return Err(err.into());
            }
        }

        Ok(BaseStream::Rustls {
            stream: SkipDebug(Box::new(StreamOwned::new(session, stream))),
            timeout,
        })
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
            #[cfg(feature = "tls-rustls")]
            BaseStream::Rustls { stream, timeout } => {
                let res = read_timeout(&mut stream.0, buf, timeout);
                handle_close_notify(res, &mut stream.0)
            }
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
            #[cfg(feature = "tls-rustls")]
            BaseStream::Rustls { stream, .. } => stream.0.write(buf),
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
            #[cfg(feature = "tls-rustls")]
            BaseStream::Rustls { stream, .. } => stream.0.flush(),
            #[cfg(test)]
            _ => Ok(()),
        }
    }
}

fn read_timeout(stream: &mut impl Read, buf: &mut [u8], timeout: &Option<mpsc::Sender<()>>) -> io::Result<usize> {
    let read = stream.read(buf)?;

    if let Some(timeout) = timeout {
        if read == 0 && !buf.is_empty() && timeout.send(()).is_err() {
            return Err(io::ErrorKind::TimedOut.into());
        }
    }

    Ok(read)
}

#[cfg(feature = "tls-rustls")]
fn handle_close_notify(
    res: io::Result<usize>,
    stream: &mut StreamOwned<ClientSession, TcpStream>,
) -> io::Result<usize> {
    match res {
        Err(err) if err.kind() == io::ErrorKind::ConnectionAborted => {
            stream.sess.send_close_notify();
            stream.sess.complete_io(&mut stream.sock)?;

            Ok(0)
        }
        res => res,
    }
}
