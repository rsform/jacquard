use crate::io;

#[cfg(not(feature = "alloc"))]
pub type Vec<T> = heapless::Vec<T, 256>;

/// Backing key-value storage trait.
/// This allows the collection type to be backed by filesystem, database, or other secondary storage.
/// Keys are intentionally just byte slices for flexibilty.
/// On no-alloc the returned vector caps out at 256 bytes and is stored on the stack.
/// Intended return path fordata is via the types that return `RW`
/// `RW`: should be a reader, preferably seekable.
pub trait Storage {
    fn get<RW>(&self, key: &[u8]) -> io::Result<Option<RW>>;
    fn get_data(&self, key: &[u8]) -> io::Result<Option<Vec<u8>>>;
    fn put_data(&self, key: &[u8], value: &[u8]) -> io::Result<()>;
    fn put<RW>(&self, key: &[u8], value: RW) -> io::Result<()>;
    fn has(&self, key: &[u8]) -> io::Result<bool>;
    fn del(&self, key: &[u8]) -> io::Result<()>;
    fn buffer(&self, key: &[u8]) -> &mut [u8];
    fn writer(&self, key: &[u8]) -> io::Result<Box<dyn io::Write>>;
}
