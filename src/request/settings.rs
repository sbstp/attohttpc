#[cfg(feature = "tls-rustls")]
use std::sync::Arc;
use std::time::Duration;

use http::HeaderMap;
#[cfg(feature = "tls-rustls")]
use rustls::ClientConfig;

#[cfg(feature = "charsets")]
use crate::charsets::Charset;
#[cfg(any(feature = "tls-rustls", feature = "tls"))]
use crate::skip_debug::SkipDebug;

#[cfg(feature = "tls")]
use native_tls::Certificate;

#[derive(Clone, Debug)]
pub struct BaseSettings {
    pub headers: HeaderMap,
    pub max_redirections: u32,
    pub follow_redirects: bool,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub timeout: Option<Duration>,
    #[cfg(feature = "charsets")]
    pub default_charset: Option<Charset>,
    #[cfg(feature = "compress")]
    pub allow_compression: bool,
    #[cfg(feature = "tls")]
    pub accept_invalid_certs: bool,
    #[cfg(feature = "tls")]
    pub accept_invalid_hostnames: bool,
    #[cfg(feature = "tls")]
    pub root_certificates: SkipDebug<Vec<Certificate>>,
    #[cfg(feature = "tls-rustls")]
    pub client_config: SkipDebug<Option<Arc<ClientConfig>>>,
}

impl Default for BaseSettings {
    fn default() -> Self {
        BaseSettings {
            headers: HeaderMap::new(),
            max_redirections: 5,
            follow_redirects: true,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(30),
            timeout: None,
            #[cfg(feature = "charsets")]
            default_charset: None,
            #[cfg(feature = "compress")]
            allow_compression: true,
            #[cfg(feature = "tls")]
            accept_invalid_certs: false,
            #[cfg(feature = "tls")]
            accept_invalid_hostnames: false,
            #[cfg(feature = "tls")]
            root_certificates: SkipDebug(Vec::new()),
            #[cfg(feature = "tls-rustls")]
            client_config: None.into(),
        }
    }
}
