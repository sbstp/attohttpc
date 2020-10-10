use std::fmt;
use std::io;
use std::io::prelude::*;
use std::sync::Arc;

use rustls::{
    ClientConfig, ClientSession, ServerCertVerified, ServerCertVerifier, Session, StreamOwned, WebPKIVerifier,
};
use webpki::DNSNameRef;
use webpki_roots::TLS_SERVER_ROOTS;

use crate::Result;

pub type Certificate = rustls::Certificate;

pub struct TlsHandshaker {
    inner: ClientConfig,
    accept_invalid_certs: bool,
    accept_invalid_hostnames: bool,
}

impl TlsHandshaker {
    pub fn new() -> TlsHandshaker {
        let mut config = ClientConfig::new();
        config.root_store.add_server_trust_anchors(&TLS_SERVER_ROOTS);

        TlsHandshaker {
            inner: config,
            accept_invalid_hostnames: false,
            accept_invalid_certs: false,
        }
    }

    pub fn danger_accept_invalid_certs(&mut self, accept_invalid_certs: bool) {
        self.accept_invalid_certs = accept_invalid_certs;
        self.update_verifier();
    }

    pub fn danger_accept_invalid_hostnames(&mut self, accept_invalid_hostnames: bool) {
        self.accept_invalid_hostnames = accept_invalid_hostnames;
        self.update_verifier();
    }

    fn update_verifier(&mut self) {
        self.inner
            .dangerous()
            .set_certificate_verifier(Arc::new(CustomCertVerifier {
                upstream: WebPKIVerifier::new(),
                accept_invalid_certs: self.accept_invalid_certs,
                accept_invalid_hostnames: self.accept_invalid_hostnames,
            }))
    }

    pub fn add_root_certificate(&mut self, cert: Certificate) -> Result<()> {
        self.inner.root_store.add(&cert)?;
        Ok(())
    }

    pub fn handshake<S>(&self, domain: &str, mut stream: S) -> Result<TlsStream<S>>
    where
        S: Read + Write,
    {
        let domain = DNSNameRef::try_from_ascii_str(domain)?;
        let config = Arc::new(self.inner.clone());
        let mut session = ClientSession::new(&config, domain);

        while let Err(err) = session.complete_io(&mut stream) {
            if err.kind() != io::ErrorKind::WouldBlock || !session.is_handshaking() {
                return Err(err.into());
            }
        }

        Ok(TlsStream {
            inner: StreamOwned::new(session, stream),
        })
    }
}

pub struct TlsStream<S>
where
    S: Read + Write,
{
    inner: StreamOwned<ClientSession, S>,
}

impl<S> TlsStream<S>
where
    S: Read + Write,
{
    fn handle_close_notify(&mut self, res: io::Result<usize>) -> io::Result<usize> {
        match res {
            Err(err) if err.kind() == io::ErrorKind::ConnectionAborted => {
                self.inner.sess.send_close_notify();
                self.inner.sess.complete_io(&mut self.inner.sock)?;

                Ok(0)
            }
            res => res,
        }
    }
}

impl<S> Read for TlsStream<S>
where
    S: Read + Write,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let res = self.inner.read(buf);
        self.handle_close_notify(res)
    }
}

impl<S> Write for TlsStream<S>
where
    S: Read + Write,
{
    #[inline]
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S> fmt::Debug for TlsStream<S>
where
    S: Read + Write,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "TlsStream[rustls]")
    }
}

struct CustomCertVerifier {
    upstream: WebPKIVerifier,
    accept_invalid_certs: bool,
    accept_invalid_hostnames: bool,
}

impl ServerCertVerifier for CustomCertVerifier {
    fn verify_server_cert(
        &self,
        roots: &rustls::RootCertStore,
        presented_certs: &[rustls::Certificate],
        dns_name: DNSNameRef,
        ocsp_response: &[u8],
    ) -> std::result::Result<rustls::ServerCertVerified, rustls::TLSError> {
        match self
            .upstream
            .verify_server_cert(roots, presented_certs, dns_name, ocsp_response)
        {
            Ok(verified) => Ok(verified),
            Err(rustls::TLSError::WebPKIError(err)) => {
                if self.accept_invalid_certs
                    || (self.accept_invalid_hostnames && err == webpki::Error::CertNotValidForName)
                {
                    return Ok(ServerCertVerified::assertion());
                }

                Err(rustls::TLSError::WebPKIError(err))
            }
            Err(err) => Err(err),
        }
    }
}
