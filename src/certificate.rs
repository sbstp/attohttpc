use std::fmt;

#[derive(Clone)]
pub struct Certificate {
    pub inner_cert: native_tls::Certificate,
}

impl fmt::Debug for Certificate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner_cert.to_der() {
            Ok(der) => formatter.write_fmt(format_args!("DER Certificate: {:?}", der)),
            Err(_) => formatter.write_str("Unknown certificate"),
        }
    }
}

impl From<native_tls::Certificate> for Certificate {
    fn from(cert: native_tls::Certificate) -> Self {
        Self { inner_cert: cert }
    }
}
