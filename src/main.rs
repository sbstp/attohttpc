#![feature(nll)]

#[macro_use]
extern crate failure;
extern crate http;
extern crate url;

mod error;

use std::fmt::{self, Display};
use std::io::prelude::*;
use std::io::{self, BufReader, BufWriter};
use std::net::TcpStream;
use std::str;

use failure::Fail;
use http::header::HOST;
use http::header::{AsHeaderName, HeaderName, HeaderValue, IntoHeaderName};
use http::{status::StatusCode, HeaderMap, HttpTryFrom, Method, Version};
use url::Url;

use error::{HttpError, HttpResult};

pub trait HttpTryInto<T> {
    fn try_into(self) -> Result<T, http::Error>;
}

impl<T, U> HttpTryInto<U> for T
where
    U: HttpTryFrom<T>,
    http::Error: std::convert::From<<U as http::HttpTryFrom<T>>::Error>,
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
    url: url::Url,
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
        let resp = self.read_request(sock)?;
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

    fn read_request(
        &mut self,
        reader: TcpStream,
    ) -> HttpResult<(StatusCode, HeaderMap, ResponseReader)> {
        let mut reader = BufReader::new(reader);
        let mut line = Vec::new();

        let mut headers = HeaderMap::new();

        // status line
        let status: StatusCode = {
            reader.read_until(b'\n', &mut line)?;
            let trimmed = trim_crlf(&line);

            let sp_1 = trimmed
                .iter()
                .position(|&c| c == b' ')
                .ok_or(HttpError::InvalidResponse)?;
            let rest = &trimmed[sp_1 + 1..];
            let sp_2 = rest
                .iter()
                .position(|&c| c == b' ')
                .ok_or(HttpError::InvalidResponse)?;

            str::from_utf8(&rest[..sp_2])?
                .parse()
                .map_err(http::Error::from)?
        };

        // headers
        loop {
            line.clear();
            reader.read_until(b'\n', &mut line)?;
            let trimmed = trim_crlf(&line);
            if trimmed.is_empty() {
                break;
            }
            let col_1 = trimmed
                .iter()
                .position(|&c| c == b':')
                .ok_or(HttpError::InvalidResponse)?;
            let header = &trimmed[..col_1];
            let value = trim_byte_left(b' ', &trimmed[col_1 + 1..]);

            headers.append(
                HeaderName::from_bytes(header).map_err(http::Error::from)?,
                HeaderValue::from_bytes(value).map_err(http::Error::from)?,
            );
        }

        Ok((status, headers, ResponseReader { inner: reader }))
    }
}

pub struct ResponseReader {
    inner: BufReader<TcpStream>,
}

impl ResponseReader {
    pub fn write_to<W>(mut self, mut writer: W) -> HttpResult<usize>
    where
        W: Write,
    {
        let mut buf = [0u8; 4096];
        let mut count = 0;
        loop {
            match self.inner.read(&mut buf)? {
                0 => break,
                n => {
                    writer.write_all(&buf[..n])?;
                    count += n;
                }
            }
        }
        Ok(count)
    }

    pub fn bytes(self) -> HttpResult<Vec<u8>> {
        let mut buf = Vec::new();
        self.write_to(&mut buf)?;
        Ok(buf)
    }

    pub fn string(self) -> HttpResult<String> {
        let buf = self.bytes()?;
        Ok(String::from_utf8(buf)?)
    }
}

fn trim_crlf(buf: &[u8]) -> &[u8] {
    if buf.ends_with(b"\r\n") {
        &buf[..buf.len() - 2]
    } else if buf.ends_with(b"\n") {
        &buf[..buf.len() - 1]
    } else {
        buf
    }
}

fn trim_byte(byte: u8, buf: &[u8]) -> &[u8] {
    trim_byte_left(byte, trim_byte_right(byte, buf))
}

fn trim_byte_left(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.first().map(|&b| b) {
        if b == byte {
            buf = &buf[1..];
        } else {
            break;
        }
    }
    buf
}

fn trim_byte_right(byte: u8, mut buf: &[u8]) -> &[u8] {
    while let Some(b) = buf.last().map(|&b| b) {
        if b == byte {
            buf = &buf[..buf.len() - 1];
        } else {
            break;
        }
    }
    buf
}

fn main() {
    let mut r = Request::new("http://sbstp.ca");
    r.param("foo", 3);
    r.param("gee", true);
    let (status, headers, reader) = r.send().unwrap();
    println!("{:?} {:#?}", status, headers);
    println!("{}", reader.string().unwrap().len());
}
