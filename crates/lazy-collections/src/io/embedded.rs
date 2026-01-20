use embedded_io::{self, ErrorType, ReadExactError, WriteFmtError};

use crate::io::{ErrorKind, Read, Seek, SeekFrom, Write};

#[derive(Clone)]
pub struct FromEmbed<T: ?Sized> {
    inner: T,
}

impl<T> FromEmbed<T> {
    /// Create a new adapter.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Consume the adapter, returning the inner object.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: ?Sized> FromEmbed<T> {
    /// Borrow the inner object.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Mutably borrow the inner object.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl embedded_io::Error for crate::io::Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        self.kind().into()
    }
}

impl<T: ?Sized> ErrorType for FromEmbed<T> {
    type Error = crate::io::Error;
}

impl<T: Read + ?Sized> embedded_io::Read for FromEmbed<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        Read::read(self.inner_mut(), buf)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ReadExactError<Self::Error>> {
        match Read::read_exact(self.inner_mut(), buf) {
            Ok(()) => Ok(()),
            Err(err) => {
                if err.kind() == ErrorKind::UnexpectedEof {
                    Err(ReadExactError::UnexpectedEof)
                } else {
                    Err(ReadExactError::Other(err))
                }
            }
        }
    }
}

impl<T: Write + ?Sized> embedded_io::Write for FromEmbed<T> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        Write::write(self.inner_mut(), buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Write::flush(self.inner_mut())
    }

    fn write_all(&mut self, buf: &[u8]) -> Result<(), Self::Error> {
        Write::write_all(self.inner_mut(), buf)
    }

    fn write_fmt(
        &mut self,
        fmt: core::fmt::Arguments<'_>,
    ) -> Result<(), embedded_io::WriteFmtError<Self::Error>> {
        Write::write_fmt(self.inner_mut(), fmt).map_err(|e| WriteFmtError::Other(e))
    }
}

impl From<embedded_io::SeekFrom> for SeekFrom {
    fn from(from: embedded_io::SeekFrom) -> Self {
        match from {
            embedded_io::SeekFrom::Start(offset) => SeekFrom::Start(offset),
            embedded_io::SeekFrom::End(offset) => SeekFrom::End(offset),
            embedded_io::SeekFrom::Current(offset) => SeekFrom::Current(offset),
        }
    }
}

impl<T: Seek + ?Sized> embedded_io::Seek for FromEmbed<T> {
    fn seek(&mut self, pos: embedded_io::SeekFrom) -> Result<u64, Self::Error> {
        Seek::seek(self.inner_mut(), pos.into())
    }

    fn rewind(&mut self) -> Result<(), Self::Error> {
        self.seek(embedded_io::SeekFrom::Start(0))?;
        Ok(())
    }

    fn stream_position(&mut self) -> Result<u64, Self::Error> {
        Seek::stream_position(self.inner_mut())
    }
}
