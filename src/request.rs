mod parse;

use std::convert::From;
use std::fmt::Display;
use std::io::{prelude::*, BufWriter};
use std::str;

use http::{
    header::{HeaderValue, IntoHeaderName, HOST},
    status::StatusCode,
    HeaderMap, HttpTryFrom, Method, Version,
};
use url::Url;

use crate::error::{HttpError, HttpResult};
use crate::tls::MaybeTls;
use parse::ResponseReader;

pub trait HttpTryInto<T> {
    fn try_into(self) -> Result<T, http::Error>;
}

impl<T, U> HttpTryInto<U> for T
where
    U: HttpTryFrom<T>,
    http::Error: From<<U as http::HttpTryFrom<T>>::Error>,
{
    fn try_into(self) -> Result<U, http::Error> {
        let val = U::try_from(self)?;
        Ok(val)
    }
}

fn header_insert<H, V>(headers: &mut HeaderMap, header: H, value: V) -> HttpResult
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.insert(header, value);
    Ok(())
}

fn header_append<H, V>(headers: &mut HeaderMap, header: H, value: V) -> HttpResult
where
    H: IntoHeaderName,
    V: HttpTryInto<HeaderValue>,
{
    let value = value.try_into()?;
    headers.append(header, value);
    Ok(())
}

pub struct Request {
    url: Url,
    method: Method,
    headers: HeaderMap,
    redirect: bool,
}

impl Request {
    pub fn new(base_url: &str) -> Request {
        let url = Url::parse(base_url).expect("invalid url");
        Request {
            url,
            method: Method::GET,
            headers: HeaderMap::new(),
            redirect: true,
        }
    }

    pub fn redirect(&mut self, redirect: bool) {
        self.redirect = redirect;
    }

    pub fn method(&mut self, method: Method) {
        self.method = method;
    }

    pub fn param<V>(&mut self, key: &str, value: V)
    where
        V: Display,
    {
        self.url
            .query_pairs_mut()
            .append_pair(key, &format!("{}", value));
    }

    pub fn header<H, V>(&mut self, header: H, value: V) -> HttpResult
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_insert(&mut self.headers, header, value)
    }

    pub fn header_append<H, V>(&mut self, header: H, value: V) -> HttpResult
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        header_append(&mut self.headers, header, value)
    }

    fn connect(&self, url: &Url) -> HttpResult<MaybeTls> {
        let host = url.host_str().ok_or(HttpError::InvalidUrl)?;
        let port = url.port_or_known_default().ok_or(HttpError::InvalidUrl)?;

        debug!("trying to connect to {}:{}", host, port);

        Ok(match url.scheme() {
            "http" => MaybeTls::connect(host, port)?,
            "https" => MaybeTls::connect_tls(host, port)?,
            _ => return Err(HttpError::InvalidUrl),
        })
    }

    fn base_redirect_url(&self, location: &str, previous_url: &Url) -> HttpResult<Url> {
        Ok(match Url::parse(location) {
            Ok(url) => url,
            Err(url::ParseError::RelativeUrlWithoutBase) => previous_url
                .join(location)
                .map_err(|_| HttpError::InvalidUrl)?,
            Err(_) => Err(HttpError::InvalidUrl)?,
        })
    }

    pub fn send(mut self) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
        let mut url = self.url.clone();
        loop {
            let mut sock = self.connect(&url)?;
            self.write_request(&mut sock, &url)?;
            let (status, headers, resp) = parse::read_response(sock)?;

            debug!("status code {}", status.as_u16());

            if !self.redirect || !status.is_redirection() {
                return Ok((status, headers, resp));
            }

            // Handle redirect
            let location = headers
                .get(http::header::LOCATION)
                .ok_or(HttpError::InvalidResponse)?;
            let location = location.to_str().map_err(|_| HttpError::InvalidResponse)?;
            let new_url = self.base_redirect_url(location, &url)?;
            url = new_url;

            debug!("redirected to {} giving url {}", location, url,);
        }
    }

    fn write_request<W>(&mut self, writer: W, url: &Url) -> HttpResult
    where
        W: Write,
    {
        let mut writer = BufWriter::new(writer);
        let version = Version::HTTP_11;

        if let Some(query) = url.query() {
            debug!(
                "{} {}?{} {:?}",
                self.method.as_str(),
                url.path(),
                query,
                version,
            );

            write!(
                writer,
                "{} {}?{} {:?}\r\n",
                self.method.as_str(),
                url.path(),
                query,
                version,
            )?;
        } else {
            debug!("{} {} {:?}", self.method.as_str(), url.path(), version);

            write!(
                writer,
                "{} {} {:?}\r\n",
                self.method.as_str(),
                url.path(),
                version,
            )?;
        }

        header_insert(&mut self.headers, "connection", "close")?;
        if let Some(domain) = url.domain() {
            header_insert(&mut self.headers, HOST, domain)?;
        }

        for (key, value) in self.headers.iter() {
            write!(writer, "{}: ", key.as_str())?;
            writer.write_all(value.as_bytes())?;
            write!(writer, "\r\n")?;
        }

        write!(writer, "\r\n")?;
        writer.flush()?;

        Ok(())
    }
}
