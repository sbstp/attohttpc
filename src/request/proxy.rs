use std::env;

use url::Url;
use wildmatch::WildMatch;

fn get_env(name: &str) -> Option<String> {
    match env::var(name.to_ascii_lowercase()).or_else(|_| env::var(name.to_ascii_uppercase())) {
        Ok(s) => Some(s),
        Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => {
            warn!(
                "Environment variable {} contains non-unicode characters",
                name.to_ascii_uppercase()
            );
            None
        }
    }
}

fn get_env_url(name: &str) -> Option<Url> {
    match get_env(name) {
        Some(val) if val.trim().is_empty() => None,
        Some(val) => match Url::parse(&val) {
            Ok(url) => match url.scheme() {
                "http" | "https" => Some(url),
                _ => {
                    warn!(
                        "Environment variable {} contains unsupported proxy scheme: {}",
                        name.to_ascii_uppercase(),
                        url.scheme()
                    );
                    None
                }
            },
            Err(err) => {
                warn!(
                    "Environment variable {} contains invalid URL: {}",
                    name.to_ascii_uppercase(),
                    err
                );
                None
            }
        },
        None => None,
    }
}

/// Contains proxy settings and utilities to find which proxy to use for a given URL.
#[derive(Clone, Debug)]
pub struct ProxySettings {
    http_proxy: Option<Url>,
    https_proxy: Option<Url>,
    no_proxy_patterns: Vec<WildMatch>,
}

impl ProxySettings {
    /// Get a new builder for ProxySettings.
    pub fn builder() -> ProxySettingsBuilder {
        ProxySettingsBuilder::new()
    }

    /// Get the proxy configuration from the environment using the `curl`/Unix proxy conventions.
    ///
    /// Only `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` are supported.
    /// `NO_PROXY` supports wildcard patterns.
    pub fn from_env() -> ProxySettings {
        let http_proxy = get_env_url("http_proxy");
        let https_proxy = get_env_url("https_proxy");
        let no_proxy = get_env("no_proxy");

        let no_proxy_patterns = no_proxy
            .map(|x| x.split(',').map(|pat| WildMatch::new(pat.trim())).collect::<Vec<_>>())
            .unwrap_or_default();

        ProxySettings {
            http_proxy,
            https_proxy,
            no_proxy_patterns,
        }
    }

    /// Get the proxy URL to use for the given URL.
    ///
    /// None is returned if there is no proxy configured for the scheme or if the hostname
    /// matches a pattern in the no proxy list.
    pub fn for_url(&self, url: &Url) -> Option<&Url> {
        if let Some(host) = url.host_str() {
            if !self.no_proxy_patterns.iter().any(|x| x.matches(host)) {
                return match url.scheme() {
                    "http" => self.http_proxy.as_ref(),
                    "https" => self.https_proxy.as_ref(),
                    _ => None,
                };
            }
        }
        None
    }
}

/// Utility to build ProxySettings easily.
#[derive(Clone, Debug)]
pub struct ProxySettingsBuilder {
    inner: ProxySettings,
}

impl ProxySettingsBuilder {
    /// Create a new ProxySetting builder with no initial configuration.
    pub fn new() -> Self {
        ProxySettingsBuilder {
            inner: ProxySettings {
                http_proxy: None,
                https_proxy: None,
                no_proxy_patterns: vec![],
            },
        }
    }

    /// Set the proxy for http requests.
    pub fn http_proxy<V>(mut self, val: V) -> Self
    where
        V: Into<Option<Url>>,
    {
        self.inner.http_proxy = val.into();
        self
    }

    /// Set the proxy for https requests.
    pub fn https_proxy<V>(mut self, val: V) -> Self
    where
        V: Into<Option<Url>>,
    {
        self.inner.https_proxy = val.into();
        self
    }

    /// Add a hostname pattern to ignore when finding the proxy to use for a URL.
    ///
    /// For instance `*.mycompany.local` will make every hostname which ends with `.mycompany.local`
    /// not go trough the proxy.
    pub fn add_no_proxy_pattern(mut self, pattern: impl AsRef<str>) -> Self {
        self.inner.no_proxy_patterns.push(WildMatch::new(pattern.as_ref()));
        self
    }

    /// Build the settings.
    pub fn build(self) -> ProxySettings {
        self.inner
    }
}

impl Default for ProxySettingsBuilder {
    fn default() -> Self {
        ProxySettingsBuilder::new()
    }
}

#[test]
fn test_proxy_for_url() {
    let s = ProxySettings {
        http_proxy: Some("http://proxy1:3128".parse().unwrap()),
        https_proxy: Some("http://proxy2:3128".parse().unwrap()),
        no_proxy_patterns: vec![WildMatch::new("*.com")],
    };

    assert_eq!(
        s.for_url(&Url::parse("http://google.ca").unwrap()),
        Some(&"http://proxy1:3128".parse().unwrap())
    );

    assert_eq!(
        s.for_url(&Url::parse("https://google.ca").unwrap()),
        Some(&"http://proxy2:3128".parse().unwrap())
    );

    assert_eq!(s.for_url(&Url::parse("https://reddit.com").unwrap()), None);
}
