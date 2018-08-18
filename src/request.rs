use std::convert::From;
use std::fmt::Display;
use std::io::{prelude::*, BufWriter};
use std::net::TcpStream;
use std::str;

use http::{
    self,
    header::{AsHeaderName, HeaderName, HeaderValue, IntoHeaderName, HOST},
    status::StatusCode,
    HeaderMap, HttpTryFrom, Method, Version,
};
use url::Url;

use error::{HttpError, HttpResult};
use parse::{self, ResponseReader};

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
}

impl Request {
    pub fn new(base_url: &str) -> Request {
        let url = Url::parse(base_url).expect("invalid url");
        Request {
            url,
            method: Method::GET,
            headers: HeaderMap::new(),
        }
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

    pub fn send(mut self) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
        let mut sock = {
            let host = self.url.host_str().ok_or(HttpError::InvalidUrl)?;
            let port = self
                .url
                .port_or_known_default()
                .ok_or(HttpError::InvalidUrl)?;
            TcpStream::connect((host, port))?
        };

        self.write_request(&mut sock)?;
        let resp = parse::read_response(sock)?;
        Ok(resp)
    }

    fn write_request<W>(&mut self, writer: W) -> HttpResult
    where
        W: Write,
    {
        let mut writer = BufWriter::new(writer);
        let version = Version::HTTP_11;

        if let Some(query) = self.url.query() {
            write!(
                writer,
                "{} {}?{} {:?}\r\n",
                self.method.as_str(),
                self.url.path(),
                query,
                version,
            )?;
        } else {
            write!(
                writer,
                "{} {} {:?}\r\n",
                self.method.as_str(),
                self.url.path(),
                version,
            )?;
        }

        header_insert(&mut self.headers, "connection", "close")?;
        if let Some(domain) = self.url.domain() {
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
