use std::convert::TryInto;
use std::io::{Result as IoResult, Write};

/// The kinds of request bodies currently supported by this crate.
#[derive(Debug, Clone, Copy)]
pub enum BodyKind {
    /// An empty request body
    Empty,
    /// A request body with a known length
    KnownLength(u64),
}

/// A generic rewindable request body
pub trait Body {
    /// Determine the kind of the request body
    fn kind(&mut self) -> IoResult<BodyKind>;

    /// Write out the request body into the given writer
    fn write<W: Write>(&mut self, writer: W) -> IoResult<()>;
}

/// An empty request body
#[derive(Debug, Clone, Copy)]
pub struct Empty;

impl Body for Empty {
    fn kind(&mut self) -> IoResult<BodyKind> {
        Ok(BodyKind::Empty)
    }

    fn write<W: Write>(&mut self, _writer: W) -> IoResult<()> {
        Ok(())
    }
}

/// A request body containing UTF-8-encoded text
#[derive(Debug, Clone)]
pub struct Text<B>(pub B);

impl<B: AsRef<str>> Body for Text<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.as_ref().len().try_into().unwrap();
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        writer.write_all(self.0.as_ref().as_bytes())
    }
}

/// A request body containing binary data
#[derive(Debug, Clone)]
pub struct Bytes<B>(pub B);

impl<B: AsRef<[u8]>> Body for Bytes<B> {
    fn kind(&mut self) -> IoResult<BodyKind> {
        let len = self.0.as_ref().len().try_into().unwrap();
        Ok(BodyKind::KnownLength(len))
    }

    fn write<W: Write>(&mut self, mut writer: W) -> IoResult<()> {
        writer.write_all(self.0.as_ref())
    }
}
