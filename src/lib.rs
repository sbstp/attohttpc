extern crate http;
extern crate url;

use std::fmt::Display;
use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::net::TcpStream;

use http::header::HOST;
use http::header::{AsHeaderName, HeaderName, HeaderValue, IntoHeaderName};
use http::{HeaderMap, HttpTryFrom, Method, Version};
use url::Url;

pub trait HttpTryInto<T> {
    type Error;
    fn try_into(self) -> Result<T, Self::Error>;
}

impl<T, U> HttpTryInto<U> for T
where
    U: HttpTryFrom<T>,
{
    type Error = <U as HttpTryFrom<T>>::Error;

    fn try_into(self) -> Result<U, Self::Error> {
        U::try_from(self)
    }
}

pub struct Request {
    url: url::Url,
    method: Method,
    version: Version,
    headers: HeaderMap,
}

impl Request {
    pub fn new(base_url: &str) -> Request {
        let url = Url::parse(base_url).expect("invalid url");
        Request {
            url,
            method: Method::GET,
            version: Version::HTTP_11,
            headers: HeaderMap::new(),
        }
    }

    pub fn method(&mut self, method: Method) {
        self.method = method;
    }

    pub fn version(&mut self, version: Version) {
        self.version = version;
    }

    pub fn param<V>(&mut self, key: &str, value: V)
    where
        V: Display,
    {
        self.url
            .query_pairs_mut()
            .append_pair(key, &format!("{}", value));
    }

    pub fn header<H, V>(
        &mut self,
        header: H,
        value: V,
    ) -> Result<(), <V as HttpTryInto<HeaderValue>>::Error>
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        self.headers.insert(header, value.try_into()?);
        Ok(())
    }

    pub fn header_append<H, V>(
        &mut self,
        header: H,
        value: V,
    ) -> Result<(), <V as HttpTryInto<HeaderValue>>::Error>
    where
        H: IntoHeaderName,
        V: HttpTryInto<HeaderValue>,
    {
        self.headers.append(header, value.try_into()?);
        Ok(())
    }

    pub fn send(mut self) -> io::Result<()> {
        let host = self.url.host_str().unwrap();
        let port = self.url.port_or_known_default().unwrap();

        let write_sock = TcpStream::connect((host, port))?;
        let read_sock = write_sock.try_clone()?;

        let mut writer = BufWriter::new(write_sock);
        let mut reader = BufReader::new(read_sock);

        if let Some(query) = self.url.query() {
            write!(
                writer,
                "{} {}?{} {:?}\r\n",
                self.method.as_str(),
                self.url.path(),
                query,
                self.version,
            )?;
        } else {
            write!(
                writer,
                "{} {} {:?}\r\n",
                self.method.as_str(),
                self.url.path(),
                self.version,
            )?;
        }

        if let Some(domain) = self.url.domain() {
            self.headers
                .insert(HOST, HeaderValue::from_str(domain).unwrap());
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
