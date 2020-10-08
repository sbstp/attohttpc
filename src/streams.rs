#[cfg(test)]
use std::io::Cursor;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::thread;

use url::{Host, Url};

use crate::happy;
use crate::parsing::buffers::BufReader2;
use crate::parsing::response::parse_response_head;
use crate::request::BaseSettings;
use crate::tls::{TlsHandshaker, TlsStream};
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
    Tls {
        stream: TlsStream<TcpStream>,
        timeout: Option<mpsc::Sender<()>>,
    },
    Tunnel {
        stream: Box<TlsStream<BufReader2<BaseStream>>>,
    },
    #[cfg(test)]
    Mock(Cursor<Vec<u8>>),
}

impl BaseStream {
    pub fn connect(info: &ConnectInfo) -> Result<BaseStream> {
        let proxy = info.base_settings.proxy_settings.for_url(info.url);
        let connect_url = proxy.unwrap_or(info.url);

        let host = connect_url.host().ok_or(ErrorKind::InvalidUrlHost)?;
        let port = connect_url.port_or_known_default().ok_or(ErrorKind::InvalidUrlPort)?;

        debug!("trying to connect to {}:{}", host, port);

        let stream = match connect_url.scheme() {
            "http" => BaseStream::connect_tcp(&host, port, info)
                .map(|(stream, timeout)| BaseStream::Plain { stream, timeout }),
            "https" => BaseStream::connect_tls(&host, port, info),
            _ => Err(ErrorKind::InvalidBaseUrl.into()),
        }?;

        if let Some(proxy_url) = proxy {
            if info.url.scheme() == "https" {
                return BaseStream::initiate_tunnel(stream, proxy_url, info.url, info.base_settings);
            }
        }

        Ok(stream)
    }

    fn initiate_tunnel(
        mut stream: BaseStream,
        proxy_url: &Url,
        remote_url: &Url,
        base_settings: &BaseSettings,
    ) -> Result<BaseStream> {
        let remote_host = remote_url.host_str().ok_or(ErrorKind::InvalidUrlHost)?;
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

        let mut handshaker = TlsHandshaker::new();
        apply_base_settings(&mut handshaker, base_settings);
        let stream = handshaker.handshake(remote_host, stream)?;

        return Ok(BaseStream::Tunnel {
            stream: Box::new(stream),
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

    fn connect_tls(host: &Host<&str>, port: u16, info: &ConnectInfo) -> Result<BaseStream> {
        let (stream, timeout) = BaseStream::connect_tcp(host, port, info)?;
        let mut handshaker = TlsHandshaker::new();
        apply_base_settings(&mut handshaker, info.base_settings);
        let stream = handshaker.handshake(&host.to_string(), stream)?;
        Ok(BaseStream::Tls { stream, timeout })
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
            BaseStream::Tls { stream, timeout } => read_timeout(stream, buf, timeout),
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
            BaseStream::Tls { stream, .. } => stream.write(buf),
            BaseStream::Tunnel { stream } => stream.write(buf),
            #[cfg(test)]
            _ => Ok(0),
        }
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        match self {
            BaseStream::Plain { stream, .. } => stream.flush(),
            BaseStream::Tls { stream, .. } => stream.flush(),
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

fn apply_base_settings(handshaker: &mut TlsHandshaker, base_settings: &BaseSettings) {
    handshaker.danger_accept_invalid_certs(base_settings.accept_invalid_certs);
    handshaker.danger_accept_invalid_hostnames(base_settings.accept_invalid_hostnames);
}
