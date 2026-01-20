use crate::io::{BufRead, Read, Seek, SeekFrom};

/// Adapter to `std::io` traits.
#[derive(Clone)]
pub struct ToStd<T: ?Sized> {
    inner: T,
}

impl<T> ToStd<T> {
    /// Create a new adapter.
    pub fn new(inner: T) -> Self {
        Self { inner }
    }

    /// Consume the adapter, returning the inner object.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T: ?Sized> ToStd<T> {
    /// Borrow the inner object.
    pub fn inner(&self) -> &T {
        &self.inner
    }

    /// Mutably borrow the inner object.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T: Read + ?Sized> std::io::Read for ToStd<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        Read::read(self.inner_mut(), buf).map_err(std::io::Error::from)
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        Read::read_to_end(self.inner_mut(), buf).map_err(std::io::Error::from)
    }

    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        Read::read_to_string(self.inner_mut(), buf).map_err(std::io::Error::from)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        Read::read_exact(self.inner_mut(), buf).map_err(std::io::Error::from)
    }
}

impl From<SeekFrom> for std::io::SeekFrom {
    fn from(from: SeekFrom) -> Self {
        match from {
            SeekFrom::Start(offset) => std::io::SeekFrom::Start(offset),
            SeekFrom::Current(offset) => std::io::SeekFrom::Current(offset),
            SeekFrom::End(offset) => std::io::SeekFrom::End(offset),
        }
    }
}

impl From<std::io::SeekFrom> for SeekFrom {
    fn from(from: std::io::SeekFrom) -> Self {
        match from {
            std::io::SeekFrom::Start(offset) => SeekFrom::Start(offset),
            std::io::SeekFrom::Current(offset) => SeekFrom::Current(offset),
            std::io::SeekFrom::End(offset) => SeekFrom::End(offset),
        }
    }
}

impl<T: Seek + ?Sized> std::io::Seek for ToStd<T> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        Seek::seek(self.inner_mut(), pos.into()).map_err(std::io::Error::from)
    }

    fn rewind(&mut self) -> std::io::Result<()> {
        self.seek(std::io::SeekFrom::Start(0))?;
        Ok(())
    }

    fn stream_position(&mut self) -> std::io::Result<u64> {
        Seek::stream_position(self.inner_mut()).map_err(std::io::Error::from)
    }

    fn seek_relative(&mut self, offset: i64) -> std::io::Result<()> {
        self.seek(std::io::SeekFrom::Current(offset))?;
        Ok(())
    }
}

impl<T: BufRead + ?Sized> std::io::BufRead for ToStd<T> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        BufRead::fill_buf(self.inner_mut()).map_err(std::io::Error::from)
    }

    fn consume(&mut self, amount: usize) {
        BufRead::consume(self.inner_mut(), amount);
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        BufRead::read_until(self.inner_mut(), byte, buf).map_err(std::io::Error::from)
    }

    fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        BufRead::read_line(self.inner_mut(), buf).map_err(std::io::Error::from)
    }
}
