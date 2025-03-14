// Copyright 2016 `multipart` Crate Developers
// Copyright 2025 Simon Bernier St-Pierre <git.sbstp.ca@gmail.com>
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! The client-side abstraction for multipart requests. Enabled with the `client` feature.
//!
//! Multipart requests which write out their data in one fell swoop.
#![allow(dead_code)]

use mime::Mime;

use std::borrow::Cow;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use std::io::prelude::*;
use std::io::Cursor;
use std::{fmt, io};

use super::{HttpRequest, HttpStream};

macro_rules! try_lazy (
    ($field:expr, $try:expr) => (
        match $try {
            Ok(ok) => ok,
            Err(e) => return Err(LazyError::with_field($field.into(), e)),
        }
    );
    ($try:expr) => (
        match $try {
            Ok(ok) => ok,
            Err(e) => return Err(LazyError::without_field(e)),
        }
    )
);

/// A `LazyError` wrapping `std::io::Error`.
pub type LazyIoError<'a> = LazyError<'a, io::Error>;

/// `Result` type for `LazyIoError`.
pub type LazyIoResult<'a, T> = Result<T, LazyIoError<'a>>;

/// An error for lazily written multipart requests, including the original error as well
/// as the field which caused the error, if applicable.
pub struct LazyError<'a, E> {
    /// The field that caused the error.
    /// If `None`, there was a problem opening the stream to write or finalizing the stream.
    pub field_name: Option<Cow<'a, str>>,
    /// The inner error.
    pub error: E,
    /// Private field for back-compat.
    _priv: (),
}

impl<'a, E> LazyError<'a, E> {
    fn without_field<E_: Into<E>>(error: E_) -> Self {
        LazyError {
            field_name: None,
            error: error.into(),
            _priv: (),
        }
    }

    fn with_field<E_: Into<E>>(field_name: Cow<'a, str>, error: E_) -> Self {
        LazyError {
            field_name: Some(field_name),
            error: error.into(),
            _priv: (),
        }
    }

    fn transform_err<E_: From<E>>(self) -> LazyError<'a, E_> {
        LazyError {
            field_name: self.field_name,
            error: self.error.into(),
            _priv: (),
        }
    }
}

/// Take `self.error`, discarding `self.field_name`.
impl<'a> From<LazyError<'a, io::Error>> for io::Error {
    fn from(val: LazyError<'a, io::Error>) -> Self {
        val.error
    }
}

impl<E: Error> Error for LazyError<'_, E> {
    fn cause(&self) -> Option<&dyn Error> {
        Some(&self.error)
    }
}

impl<E: fmt::Debug> fmt::Debug for LazyError<'_, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref field_name) = self.field_name {
            fmt.write_fmt(format_args!("LazyError (on field {:?}): {:?}", field_name, self.error))
        } else {
            fmt.write_fmt(format_args!("LazyError (misc): {:?}", self.error))
        }
    }
}

impl<E: fmt::Display> fmt::Display for LazyError<'_, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref field_name) = self.field_name {
            fmt.write_fmt(format_args!("Error writing field {:?}: {}", field_name, self.error))
        } else {
            fmt.write_fmt(format_args!("Error opening or flushing stream: {}", self.error))
        }
    }
}

/// A multipart request which writes all fields at once upon being provided an output stream.
///
/// Sacrifices static dispatch for support for dynamic construction. Reusable.
///
/// #### Lifetimes
/// * `'n`: Lifetime for field **n**ames; will only escape this struct in `LazyIoError<'n>`.
/// * `'d`: Lifetime for **d**ata: will only escape this struct in `PreparedFields<'d>`.
#[derive(Debug, Default)]
pub struct Multipart<'n, 'd> {
    fields: Vec<Field<'n, 'd>>,
}

impl<'n, 'd> Multipart<'n, 'd> {
    /// Initialize a new lazy dynamic request.
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a text field to this request.
    pub fn add_text<N, T>(&mut self, name: N, text: T) -> &mut Self
    where
        N: Into<Cow<'n, str>>,
        T: Into<Cow<'d, str>>,
    {
        self.fields.push(Field {
            name: name.into(),
            data: Data::Text(text.into()),
        });

        self
    }

    /// Add a file field to this request.
    ///
    /// ### Note
    /// Does not check if `path` exists.
    pub fn add_file<N, P>(&mut self, name: N, path: P) -> &mut Self
    where
        N: Into<Cow<'n, str>>,
        P: IntoCowPath<'d>,
    {
        self.fields.push(Field {
            name: name.into(),
            data: Data::File(path.into_cow_path()),
        });

        self
    }

    /// Add a generic stream field to this request,
    pub fn add_stream<N, R, F>(&mut self, name: N, stream: R, filename: Option<F>, mime: Option<Mime>) -> &mut Self
    where
        N: Into<Cow<'n, str>>,
        R: Read + 'd,
        F: Into<Cow<'n, str>>,
    {
        self.fields.push(Field {
            name: name.into(),
            data: Data::Stream(Stream {
                content_type: mime.unwrap_or(mime::APPLICATION_OCTET_STREAM),
                filename: filename.map(|f| f.into()),
                stream: Box::new(stream),
            }),
        });

        self
    }

    /// Convert `req` to `HttpStream`, write out the fields in this request, and finish the
    /// request, returning the response if successful, or the first error encountered.
    ///
    /// If any files were added by path they will now be opened for reading.
    pub fn send<R: HttpRequest>(
        &mut self,
        mut req: R,
    ) -> Result<<R::Stream as HttpStream>::Response, LazyError<'n, <R::Stream as HttpStream>::Error>> {
        let mut prepared = self.prepare().map_err(LazyError::transform_err)?;

        req.apply_headers(prepared.boundary(), prepared.content_len());

        let mut stream = try_lazy!(req.open_stream());

        try_lazy!(io::copy(&mut prepared, &mut stream));

        stream.finish().map_err(LazyError::without_field)
    }

    /// Export the multipart data contained in this lazy request as an adaptor which implements `Read`.
    ///
    /// During this step, if any files were added by path then they will be opened for reading
    /// and their length measured.
    pub fn prepare(&mut self) -> LazyIoResult<'n, PreparedFields<'d>> {
        PreparedFields::from_fields(&mut self.fields)
    }
}

#[derive(Debug)]
struct Field<'n, 'd> {
    name: Cow<'n, str>,
    data: Data<'n, 'd>,
}

enum Data<'n, 'd> {
    Text(Cow<'d, str>),
    File(Cow<'d, Path>),
    Stream(Stream<'n, 'd>),
}

impl fmt::Debug for Data<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Data::Text(ref text) => write!(f, "Data::Text({:?})", text),
            Data::File(ref path) => write!(f, "Data::File({:?})", path),
            Data::Stream(_) => f.write_str("Data::Stream(Box<Read>)"),
        }
    }
}

struct Stream<'n, 'd> {
    filename: Option<Cow<'n, str>>,
    content_type: Mime,
    stream: Box<dyn Read + 'd>,
}

/// The result of [`Multipart::prepare()`](struct.Multipart.html#method.prepare).
///
/// Implements `Read`, contains the entire request body.
///
/// Individual files/streams are dropped as they are read to completion.
///
/// ### Note
/// The fields in the request may have been reordered to simplify the preparation step.
/// No compliant server implementation will be relying on the specific ordering of fields anyways.
pub struct PreparedFields<'d> {
    text_data: Cursor<Vec<u8>>,
    streams: Vec<PreparedField<'d>>,
    end_boundary: Cursor<String>,
    content_len: Option<u64>,
}

impl<'d> PreparedFields<'d> {
    fn from_fields<'n>(fields: &mut Vec<Field<'n, 'd>>) -> Result<Self, LazyIoError<'n>> {
        debug!("Field count: {}", fields.len());

        // One of the two RFCs specifies that any bytes before the first boundary are to be
        // ignored anyway
        let mut boundary = format!("\r\n--{}", super::gen_boundary());

        let mut text_data = Vec::new();
        let mut streams = Vec::new();
        let mut content_len = 0u64;
        let mut use_len = true;

        for field in fields.drain(..) {
            match field.data {
                Data::Text(text) => write!(
                    text_data,
                    "{}\r\nContent-Disposition: form-data; \
                     name=\"{}\"\r\n\r\n{}",
                    boundary, field.name, text
                )
                .unwrap(),
                Data::File(file) => {
                    let (stream, len) = PreparedField::from_path(field.name, &file, &boundary)?;
                    content_len += len;
                    streams.push(stream);
                }
                Data::Stream(stream) => {
                    use_len = false;

                    streams.push(PreparedField::from_stream(
                        &field.name,
                        &boundary,
                        &stream.content_type,
                        stream.filename.as_deref(),
                        stream.stream,
                    ));
                }
            }
        }

        // So we don't write a spurious end boundary
        if text_data.is_empty() && streams.is_empty() {
            boundary = String::new();
        } else {
            boundary.push_str("--");
        }

        content_len += boundary.len() as u64;

        Ok(PreparedFields {
            text_data: Cursor::new(text_data),
            streams,
            end_boundary: Cursor::new(boundary),
            content_len: if use_len { Some(content_len) } else { None },
        })
    }

    /// Get the content-length value for this set of fields, if applicable (all fields are sized,
    /// i.e. not generic streams).
    pub fn content_len(&self) -> Option<u64> {
        self.content_len
    }

    /// Get the boundary that was used to serialize the request.
    pub fn boundary(&self) -> &str {
        let boundary = self.end_boundary.get_ref();

        // Get just the bare boundary string
        &boundary[4..boundary.len() - 2]
    }
}

impl Read for PreparedFields<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            debug!("PreparedFields::read() was passed a zero-sized buffer.");
            return Ok(0);
        }

        let mut total_read = 0;

        while total_read < buf.len() && !cursor_at_end(&self.end_boundary) {
            let buf = &mut buf[total_read..];

            total_read += if !cursor_at_end(&self.text_data) {
                self.text_data.read(buf)?
            } else if let Some(mut field) = self.streams.pop() {
                match field.read(buf) {
                    Ok(0) => continue,
                    res => {
                        self.streams.push(field);
                        res
                    }
                }?
            } else {
                self.end_boundary.read(buf)?
            };
        }

        Ok(total_read)
    }
}

struct PreparedField<'d> {
    header: Cursor<Vec<u8>>,
    stream: Box<dyn Read + 'd>,
}

impl<'d> PreparedField<'d> {
    fn from_path<'n>(name: Cow<'n, str>, path: &Path, boundary: &str) -> Result<(Self, u64), LazyIoError<'n>> {
        let (content_type, filename) = super::mime_filename(path);

        let file = try_lazy!(name, File::open(path));
        let content_len = try_lazy!(name, file.metadata()).len();

        let stream = Self::from_stream(&name, boundary, &content_type, filename, Box::new(file));

        let content_len = content_len + (stream.header.get_ref().len() as u64);

        Ok((stream, content_len))
    }

    fn from_stream(
        name: &str,
        boundary: &str,
        content_type: &Mime,
        filename: Option<&str>,
        stream: Box<dyn Read + 'd>,
    ) -> Self {
        let mut header = Vec::new();

        write!(
            header,
            "{}\r\nContent-Disposition: form-data; name=\"{}\"",
            boundary, name
        )
        .unwrap();

        if let Some(filename) = filename {
            write!(header, "; filename=\"{}\"", filename).unwrap();
        }

        write!(header, "\r\nContent-Type: {}\r\n\r\n", content_type).unwrap();

        PreparedField {
            header: Cursor::new(header),
            stream,
        }
    }
}

impl Read for PreparedField<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        debug!("PreparedField::read()");

        if !cursor_at_end(&self.header) {
            self.header.read(buf)
        } else {
            self.stream.read(buf)
        }
    }
}

impl fmt::Debug for PreparedField<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("PreparedField")
            .field("header", &self.header)
            .field("stream", &"Box<Read>")
            .finish()
    }
}

/// Conversion trait necessary for `Multipart::add_file()` to accept borrowed or owned strings
/// and borrowed or owned paths
pub trait IntoCowPath<'a> {
    /// Self-explanatory, hopefully
    fn into_cow_path(self) -> Cow<'a, Path>;
}

impl<'a> IntoCowPath<'a> for Cow<'a, Path> {
    fn into_cow_path(self) -> Cow<'a, Path> {
        self
    }
}

impl IntoCowPath<'static> for PathBuf {
    fn into_cow_path(self) -> Cow<'static, Path> {
        self.into()
    }
}

impl<'a> IntoCowPath<'a> for &'a Path {
    fn into_cow_path(self) -> Cow<'a, Path> {
        self.into()
    }
}

impl IntoCowPath<'static> for String {
    fn into_cow_path(self) -> Cow<'static, Path> {
        PathBuf::from(self).into()
    }
}

impl<'a> IntoCowPath<'a> for &'a str {
    fn into_cow_path(self) -> Cow<'a, Path> {
        Path::new(self).into()
    }
}

fn cursor_at_end<T: AsRef<[u8]>>(cursor: &Cursor<T>) -> bool {
    cursor.position() == (cursor.get_ref().as_ref().len() as u64)
}
