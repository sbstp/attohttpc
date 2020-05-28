use super::body::{Body, BodyKind};
use super::{Error, ErrorKind, Result};
use mime::Mime;
use multipart::client as mp;
use std::fmt;
use std::io::{copy, prelude::*, Cursor, Error as IoError, Result as IoResult};

/// A file to be uploaded as part of a multipart form.
#[derive(Debug, Clone)]
pub struct MultipartFile {
    name: String,
    file: Vec<u8>,
    filename: Option<String>,
    mime: Option<Mime>,
}

impl MultipartFile {
    /// Constructs a new `MultipartFile` from the name and contents.
    pub fn new(name: impl AsRef<str>, file: impl AsRef<[u8]>) -> Self {
        let name = name.as_ref().to_owned();
        let file = file.as_ref().to_owned();
        Self {
            name,
            file,
            filename: None,
            mime: None,
        }
    }

    /// Sets the MIME type of the file.
    ///
    /// # Errors
    /// Returns an error if the MIME type is invalid.
    pub fn with_type(self, mime_type: impl AsRef<str>) -> Result<Self> {
        let mime_str = mime_type.as_ref();
        let mime: Mime = match mime_str.parse() {
            Ok(mime) => mime,
            Err(()) => return Err(Error(Box::new(ErrorKind::InvalidMimeType(mime_str.to_string())))),
        };
        Ok(Self {
            mime: Some(mime),
            ..self
        })
    }

    /// Sets the filename of the file.
    pub fn with_filename(self, filename: impl AsRef<str>) -> Self {
        Self {
            filename: Some(filename.as_ref().to_owned()),
            ..self
        }
    }
}

/// A builder for creating a `Multipart` body.
#[derive(Debug, Clone, Default)]
pub struct MultipartBuilder {
    text: Vec<(String, String)>,
    files: Vec<MultipartFile>,
}

impl MultipartBuilder {
    /// Creates a new `MultipartBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a text field to the form.
    pub fn with_text(mut self, name: impl AsRef<str>, text: impl AsRef<str>) -> Self {
        let name = name.as_ref().to_string();
        let text = text.as_ref().to_string();
        self.text.push((name, text));
        self
    }

    /// Adds a `MultipartFile` to the form.
    pub fn with_file(mut self, file: MultipartFile) -> Self {
        self.files.push(file);
        self
    }

    /// Creates a `Multipart` to be used as a body.
    pub fn build(self) -> Result<Multipart> {
        let mut mp = mp::lazy::Multipart::new();
        for (k, v) in self.text {
            mp.add_text(k, v);
        }
        for file in self.files {
            mp.add_stream(file.name, Cursor::new(file.file), file.filename, file.mime);
        }
        let prepared = mp.prepare().map_err::<IoError, _>(Into::into)?;
        Ok(Multipart { data: prepared })
    }
}

/// A multipart form created using `MultipartBuilder`.
pub struct Multipart {
    data: mp::lazy::PreparedFields<'static>,
}

impl Body for Multipart {
    fn kind(&mut self) -> IoResult<BodyKind> {
        match self.data.content_len() {
            Some(len) => Ok(BodyKind::KnownLength(len)),
            None => Ok(BodyKind::Chunked),
        }
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        copy(&mut self.data, &mut writer)?;
        Ok(())
    }

    fn content_type(&mut self) -> IoResult<Option<String>> {
        Ok(Some(format!("multipart/form-data; boundary={}", self.data.boundary())))
    }
}

impl fmt::Debug for Multipart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Multipart").finish()
    }
}
