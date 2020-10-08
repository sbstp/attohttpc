#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
use std::sync::Arc;
use std::thread;

#[cfg(feature = "tls")]
use native_tls::{HandshakeError, TlsConnector, TlsStream};
#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
use rustls::{ClientConfig, ClientSession, Session, StreamOwned};
use url::{Host, Url};
#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
use webpki::DNSNameRef;
#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
use webpki_roots::TLS_SERVER_ROOTS;

use crate::happy;
#[cfg(any(feature = "tls", feature = "tls-rustls"))]
use crate::parsing::buffers::BufReader2;
#[cfg(any(feature = "tls", feature = "tls-rustls"))]
use crate::parsing::response::parse_response_head;
use crate::request::BaseSettings;
#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
use crate::skip_debug::SkipDebug;
use crate::{ErrorKind, Result};

pub struct ConnectInfo<'a> {
    pub url: &'a Url,
    pub base_settings: &'a BaseSettings,
}

#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
type RustlsStream<T> = SkipDebug<StreamOwned<ClientSession, T>>;

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
    #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
    Rustls {
        stream: RustlsStream<TcpStream>,
        timeout: Option<mpsc::Sender<()>>,
    },
    #[cfg(any(feature = "tls", feature = "tls-rustls"))]
    Tunnel {
        #[cfg(feature = "tls")]
        stream: Box<TlsStream<BufReader2<BaseStream>>>,
        #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
        stream: Box<RustlsStream<BufReader2<BaseStream>>>,
    },
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(info: &ConnectInfo) -> Result<BaseStream> {
        let proxy = info.base_settings.proxy_settings.for_url(info.url);
        let connect_url = proxy.unwrap_or(info.url);

        // If TLS is not enabled by cargo features, we cannot support
        // https tunnels.
        #[cfg(all(not(feature = "tls"), not(feature = "tls-rustls")))]
        if proxy.map(|x| x.scheme()) == Some("https") {
            return Err(ErrorKind::InvalidProxy.into());
        }

        let host = connect_url.host().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = connect_url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        let stream = match connect_url.scheme() {
            "http" => BaseStream::connect_tcp(&host, port, info)
                .map(|(stream, timeout)| BaseStream::Plain { stream, timeout }),
            #[cfg(any(feature = "tls", feature = "tls-rustls"))]
            "https" => BaseStream::connect_tls(&host, port, info),
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }?;

        #[cfg(any(feature = "tls", feature = "tls-rustls"))]
        if let Some(proxy_url) = proxy {
            if info.url.scheme() == "https" {
                return BaseStream::initiate_tunnel(stream, proxy_url, info.url, info.base_settings);
            }
        }

        Ok(stream)
    }

    #[cfg(any(feature = "tls", feature = "tls-rustls"))]
    fn initiate_tunnel(
        mut stream: BaseStream,
        proxy_url: &Url,
        remote_url: &Url,
        base_settings: &BaseSettings,
    ) -> Result<BaseStream> {
        let remote_host = remote_url.host().ok_or(ErrorKind::InvalidUrlHost)?;
        let remote_port = remote_url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;
        let proxy_host = proxy_url.host().ok_or(ErrorKind::InvalidUrlHost)?;
        let proxy_port = proxy_url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!(
            "tunnelling to {}:{} via {}:{}",
            remote_host, remote_port, proxy_host, proxy_port,
        );

        write!(stream, "CONNECT {}:{} HTTP/1.1\r\n", remote_host, remote_port)?;
        write!(stream, "Host: {}:{}\r\n\r\n", proxy_host, proxy_port)?;

        let mut stream = BufReader2::new(stream);
        let (status, _) = parse_response_head(&mut stream)?;

        if !status.is_success() {
            // TODO improve error handling
            let mut buf = String::new();
            stream.read_to_string(&mut buf).unwrap();
            println!("{} -- {}", status, buf);
            return Err(ErrorKind::ConnectError.into());
        }

        let stream = BaseStream::handshake_tls(&remote_host, &base_settings, stream)?;

        return Ok(BaseStream::Tunnel {
            #[cfg(feature = "tls")]
            stream: Box::new(stream),
            #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
            stream: Box::new(SkipDebug(stream)),
        });
    }

    fn connect_tcp(host: &Host<&str>, port: u16, info: &ConnectInfo) -> Result<(TcpStream, Option<mpsc::Sender<()>>)> {
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
    fn connect_tls(host: &Host<&str>, port: u16, info: &ConnectInfo) -> Result<BaseStream> {
        let (stream, timeout) = BaseStream::connect_tcp(host, port, info)?;
        let stream = BaseStream::handshake_tls(host, info.base_settings, stream)?;
        Ok(BaseStream::Tls { stream, timeout })
    }

    #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
    fn connect_tls(host: &Host<&str>, port: u16, info: &ConnectInfo) -> Result<BaseStream> {
        let (stream, timeout) = BaseStream::connect_tcp(host, port, info)?;
        let stream = BaseStream::handshake_tls(host, info.base_settings, stream)?;
        Ok(BaseStream::Rustls {
            stream: SkipDebug(stream),
            timeout,
        })
    }

    #[cfg(feature = "tls")]
    fn handshake_tls<S>(host: &Host<&str>, base_settings: &BaseSettings, stream: S) -> Result<TlsStream<S>>
    where
        S: Read + Write,
    {
        let mut connector_builder = TlsConnector::builder();
        connector_builder.danger_accept_invalid_certs(base_settings.accept_invalid_certs);
        connector_builder.danger_accept_invalid_hostnames(base_settings.accept_invalid_hostnames);
        for cert in &base_settings.root_certificates.0 {
            connector_builder.add_root_certificate(cert.clone());
        }
        let connector = connector_builder.build()?;
        let host_str = host.to_string(); // TODO check domain vs IP
        let stream = match connector.connect(&host_str, stream) {
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

        Ok(stream)
    }

    #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
    fn handshake_tls<S>(
        host: &Host<&str>,
        base_settings: &BaseSettings,
        mut stream: S,
    ) -> Result<StreamOwned<ClientSession, S>>
    where
        S: Read + Write,
    {
        let host_str = host.to_string();
        let name = DNSNameRef::try_from_ascii_str(&host_str)?;

        let mut session = match &base_settings.client_config.0 {
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

        Ok(StreamOwned::new(session, stream))
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
            #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
            BaseStream::Rustls { stream, timeout } => {
                let res = read_timeout(&mut stream.0, buf, timeout);
                handle_close_notify(res, &mut stream.0)
            }
            #[cfg(any(feature = "tls", feature = "tls-rustls"))]
            BaseStream::Tunnel { stream } => stream.read(buf),
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
            #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
            BaseStream::Rustls { stream, .. } => stream.0.write(buf),
            #[cfg(any(feature = "tls", feature = "tls-rustls"))]
            BaseStream::Tunnel { stream } => stream.write(buf),
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
            #[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
            BaseStream::Rustls { stream, .. } => stream.0.flush(),
            #[cfg(any(feature = "tls", feature = "tls-rustls"))]
            BaseStream::Tunnel { stream } => stream.flush(),
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

#[cfg(all(feature = "tls-rustls", not(feature = "tls")))]
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
