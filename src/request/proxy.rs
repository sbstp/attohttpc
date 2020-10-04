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

/// Contains proxy settings and utilities to find which proxy to use for a given URL.
#[derive(Clone, Debug)]
pub struct ProxySettings {
    http_proxy: Option<String>,
    https_proxy: Option<String>,
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
        let http_proxy = get_env("http_proxy");
        let https_proxy = get_env("https_proxy");
        let no_proxy = get_env("no_proxy");

        let no_proxy_patterns = no_proxy
            .map(|x| x.split(",").map(|pat| WildMatch::new(pat.trim())).collect::<Vec<_>>())
            .unwrap_or_default();

        ProxySettings {
            http_proxy,
            https_proxy,
            no_proxy_patterns,
        }
    }

    /// Get the proxy URL to use for the given URL.
    ///
    /// None is returned if there is no proxy configured for the scheme of if the hostname
    /// matches a pattern in the no proxy list.
    pub fn for_url(&self, url: &Url) -> Option<&str> {
        if let Some(host) = url.host_str() {
            if !self.no_proxy_patterns.iter().any(|x| x.is_match(host)) {
                return match url.scheme() {
                    "http" => self.http_proxy.as_ref().map(|x| x.as_str()),
                    "https" => self.https_proxy.as_ref().map(|x| x.as_str()),
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
    pub fn http_proxy<V, S>(mut self, val: V) -> Self
    where
        V: Into<Option<S>>,
        S: Into<String>,
    {
        self.inner.http_proxy = val.into().map(|x| x.into());
        self
    }

    /// Set the proxy for https requests.
    pub fn https_proxy<V, S>(mut self, val: V) -> Self
    where
        V: Into<Option<S>>,
        S: Into<String>,
    {
        self.inner.https_proxy = val.into().map(|x| x.into());
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
}

#[test]
fn test_proxy_for_url() {
    let s = ProxySettings {
        http_proxy: Some("http://proxy1:3128".into()),
        https_proxy: Some("http://proxy2:3128".into()),
        no_proxy_patterns: vec![WildMatch::new("*.com")],
    };

    assert_eq!(
        s.for_url(&Url::parse("http://google.ca").unwrap()),
        Some("http://proxy1:3128")
    );

    assert_eq!(
        s.for_url(&Url::parse("https://google.ca").unwrap()),
        Some("http://proxy2:3128")
    );

    assert_eq!(s.for_url(&Url::parse("https://reddit.com").unwrap()), None);
}
