use std::time::Duration;

use http::HeaderMap;

#[cfg(feature = "charsets")]
use crate::charsets::Charset;
use crate::request::proxy::ProxySettings;
use crate::skip_debug::SkipDebug;
use crate::tls::Certificate;

#[derive(Clone, Debug)]
pub struct BaseSettings {
    pub headers: HeaderMap,
    pub max_headers: usize,
    pub max_redirections: u32,
    pub follow_redirects: bool,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub timeout: Option<Duration>,
    pub proxy_settings: ProxySettings,
    pub accept_invalid_certs: bool,
    pub accept_invalid_hostnames: bool,
    pub root_certificates: SkipDebug<Vec<Certificate>>,

    #[cfg(feature = "charsets")]
    pub default_charset: Option<Charset>,
    #[cfg(feature = "flate2")]
    pub allow_compression: bool,
}

impl Default for BaseSettings {
    fn default() -> Self {
        BaseSettings {
            headers: HeaderMap::new(),
            max_headers: 100,
            max_redirections: 5,
            follow_redirects: true,
            connect_timeout: Duration::from_secs(30),
            read_timeout: Duration::from_secs(30),
            timeout: None,
            proxy_settings: ProxySettings::from_env(),
            accept_invalid_certs: false,
            accept_invalid_hostnames: false,
            root_certificates: SkipDebug(Vec::new()),

            #[cfg(feature = "charsets")]
            default_charset: None,
            #[cfg(feature = "flate2")]
            allow_compression: true,
        }
    }
}
