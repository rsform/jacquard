// Buffered reader types, pulling HEAVILY from the std library and tokio
// Single reader supporting sync and async traits

use core::pin::Pin;

pub use buffer::BorrowedCursor;
pub use buffer::Buffer;

use crate::io::{self, BufRead, DEFAULT_BUF_SIZE, ErrorKind, Read, Seek, SeekFrom};

/// The `BufReader` struct adds buffering to any reader.
///
/// It can be excessively inefficient to work directly with a [`AsyncRead`]
/// instance. A `BufReader` performs large, infrequent reads on the underlying
/// [`AsyncRead`] and maintains an in-memory buffer of the results.
///
/// `BufReader` can improve the speed of programs that make *small* and
/// *repeated* read calls to the same file or network socket. It does not
/// help when reading very large amounts at once, or reading just one or a few
/// times. It also provides no advantage when reading from a source that is
/// already in memory, like a `Vec<u8>`.
///
/// When the `BufReader` is dropped, the contents of its buffer will be
/// discarded. Creating multiple instances of a `BufReader` on the same
/// stream can cause data loss.
#[pin_project::pin_project]
pub struct BufReader<R: ?Sized> {
    // for async buffered read ops
    pub(super) seek_state: SeekState,
    pub(super) buf: Buffer,
    #[pin]
    pub(super) inner: R,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum SeekState {
    /// `start_seek` has not been called.
    Init,
    /// `start_seek` has been called, but `poll_complete` has not yet been called.
    Start(SeekFrom),
    /// Waiting for completion of the first `poll_complete` in the `n.checked_sub(remainder).is_none()` branch.
    PendingOverflowed(i64),
    /// Waiting for completion of `poll_complete`.
    Pending,
}

impl<R: Read> BufReader<R> {
    /// Creates a new `BufReader` with a default buffer capacity. The default is currently 8 KB,
    /// but may change in the future.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader` with the specified buffer capacity.
    pub fn with_capacity(capacity: usize, inner: R) -> Self {
        Self {
            inner,
            buf: Buffer::with_capacity(capacity),
            seek_state: SeekState::Init,
        }
    }
}

impl<R: ?Sized> BufReader<R> {
    /// Gets a reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Returns a reference to the internally buffered data.
    ///
    /// Unlike `fill_buf`, this will not attempt to fill the buffer if it is empty.
    pub fn buffer(&self) -> &[u8] {
        &self.buf.buffer()[self.buf.pos()..self.buf.capacity()]
    }

    /// Consumes this `BufReader`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> R
    where
        R: Sized,
    {
        self.inner
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer_pinned(self: Pin<&mut Self>) {
        let me = self.project();
        me.buf.discard_buffer();
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer(&mut self) {
        self.buf.discard_buffer();
    }

    pub fn capacity(&self) -> usize {
        self.buf.capacity()
    }
}

impl<R: Read + ?Sized> BufReader<R> {
    pub fn peek(&mut self, n: usize) -> io::Result<&[u8]> {
        assert!(n <= self.capacity());
        while n > self.buf.buffer().len() {
            if self.buf.pos() > 0 {
                self.buf.backshift();
            }
            let new = self.buf.read_more(&mut self.inner)?;
            if new == 0 {
                // end of file, no more bytes to read
                return Ok(&self.buf.buffer()[..]);
            }
            debug_assert_eq!(self.buf.pos(), 0);
        }
        Ok(&self.buf.buffer()[..n])
    }
}

impl<R: Read + ?Sized> Read for BufReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.buf.pos() == self.buf.filled() && buf.len() >= self.capacity() {
            self.discard_buffer();
            return self.inner.read(buf);
        }
        let mut rem = self.fill_buf()?;
        let nread = rem.read(buf)?;
        self.consume(nread);
        Ok(nread)
    }

    fn read_exact(&mut self, mut buf: &mut [u8]) -> io::Result<()> {
        if self
            .buf
            .consume_with(buf.len(), |claimed| buf.copy_from_slice(claimed))
        {
            return Ok(());
        }
        while !buf.is_empty() {
            match self.read(buf) {
                Ok(0) => break,
                Ok(n) => {
                    buf = &mut buf[n..];
                }
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
        if !buf.is_empty() {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer".into(),
            ))
        } else {
            Ok(())
        }
    }
}

impl<R: Read + ?Sized> BufRead for BufReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.buf.fill_buf(&mut self.inner)
    }

    fn consume(&mut self, amt: usize) {
        self.buf.consume(amt)
    }
}

impl<R: ?Sized + Seek> BufReader<R> {
    /// Seeks relative to the current position. If the new position lies within the buffer,
    /// the buffer will not be flushed, allowing for more efficient seeks.
    /// This method does not return the location of the underlying reader, so the caller
    /// must track this information themselves if it is required.
    pub fn seek_relative(&mut self, offset: i64) -> io::Result<()> {
        let pos = self.buf.pos() as u64;
        if offset < 0 {
            if let Some(_) = pos.checked_sub((-offset) as u64) {
                self.buf.unconsume((-offset) as usize);
                return Ok(());
            }
        } else if let Some(new_pos) = pos.checked_add(offset as u64) {
            if new_pos <= self.buf.filled() as u64 {
                self.buf.consume(offset as usize);
                return Ok(());
            }
        }

        self.seek(SeekFrom::Current(offset)).map(drop)
    }
}

impl<R: ?Sized + Seek> Seek for BufReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let result: u64;
        if let SeekFrom::Current(n) = pos {
            let remainder = (self.buf.filled() - self.buf.pos()) as i64;
            // it should be safe to assume that remainder fits within an i64 as the alternative
            // means we managed to allocate 8 exbibytes and that's absurd.
            // But it's not out of the realm of possibility for some weird underlying reader to
            // support seeking by i64::MIN so we need to handle underflow when subtracting
            // remainder.
            if let Some(offset) = n.checked_sub(remainder) {
                result = self.inner.seek(SeekFrom::Current(offset))?;
            } else {
                // seek backwards by our remainder, and then by the offset
                self.inner.seek(SeekFrom::Current(-remainder))?;
                self.discard_buffer();
                result = self.inner.seek(SeekFrom::Current(n))?;
            }
        } else {
            // Seeking with Start/End doesn't care about our buffer length.
            result = self.inner.seek(pos)?;
        }
        self.discard_buffer();
        Ok(result)
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        let remainder = (self.buf.filled() - self.buf.pos()) as u64;
        self.inner.stream_position().map(|pos| {
            pos.checked_sub(remainder).expect(
                "overflow when subtracting remaining buffer size from inner stream position",
            )
        })
    }
}

mod buffer {
    //! An encapsulation of `BufReader`'s buffer management logic.
    //!
    //! This module factors out the basic functionality of `BufReader` in order to protect two core
    //! invariants:
    //! * `filled` bytes of `buf` are always initialized
    //! * `pos` is always <= `filled`
    //! Since this module encapsulates the buffer management logic, we can ensure that the range
    //! `pos..filled` is always a valid index into the initialized region of the buffer. This means
    //! that user code which wants to do reads from a `BufReader` via `buffer` + `consume` can do so
    //! without encountering any runtime bounds checks.

    use crate::io::{self, Read};
    use core::mem::MaybeUninit;
    use core::{cmp, fmt};

    pub struct Buffer {
        // The buffer.
        buf: Box<[MaybeUninit<u8>]>,
        // The current seek offset into `buf`, must always be <= `filled`.
        pos: usize,
        // Each call to `fill_buf` sets `filled` to indicate how many bytes at the start of `buf` are
        // initialized with bytes from a read.
        filled: usize,
        // This is the max number of bytes returned across all `fill_buf` calls. We track this so that we
        // can accurately tell `read_buf` how many bytes of buf are initialized, to bypass as much of its
        // defensive initialization as possible. Note that while this often the same as `filled`, it
        // doesn't need to be. Calls to `fill_buf` are not required to actually fill the buffer, and
        // omitting this is a huge perf regression for `Read` impls that do not.
        initialized: usize,
    }

    impl Buffer {
        #[inline]
        pub fn with_capacity(capacity: usize) -> Self {
            let buf = Box::new_uninit_slice(capacity);
            Self {
                buf,
                pos: 0,
                filled: 0,
                initialized: 0,
            }
        }

        #[inline]
        pub fn buffer(&self) -> &[u8] {
            // SAFETY: self.pos and self.filled are valid, and self.filled >= self.pos, and
            // that region is initialized because those are all invariants of this type.
            unsafe {
                self.buf
                    .get_unchecked(self.pos..self.filled)
                    .assume_init_ref()
            }
        }

        #[inline]
        pub fn capacity(&self) -> usize {
            self.buf.len()
        }

        #[inline]
        pub fn filled(&self) -> usize {
            self.filled
        }

        #[inline]
        pub fn pos(&self) -> usize {
            self.pos
        }

        // This is only used by a test which asserts that the initialization-tracking is correct.
        #[cfg(test)]
        pub fn initialized(&self) -> usize {
            self.initialized
        }

        #[inline]
        pub fn discard_buffer(&mut self) {
            self.pos = 0;
            self.filled = 0;
        }

        #[inline]
        pub fn consume(&mut self, amt: usize) {
            self.pos = cmp::min(self.pos + amt, self.filled);
        }

        /// If there are `amt` bytes available in the buffer, pass a slice containing those bytes to
        /// `visitor` and return true. If there are not enough bytes available, return false.
        #[inline]
        pub fn consume_with<V>(&mut self, amt: usize, mut visitor: V) -> bool
        where
            V: FnMut(&[u8]),
        {
            if let Some(claimed) = self.buffer().get(..amt) {
                visitor(claimed);
                // If the indexing into self.buffer() succeeds, amt must be a valid increment.
                self.pos += amt;
                true
            } else {
                false
            }
        }

        #[inline]
        pub fn unconsume(&mut self, amt: usize) {
            self.pos = self.pos.saturating_sub(amt);
        }

        /// Read more bytes into the buffer without discarding any of its contents
        pub fn read_more(&mut self, mut reader: impl Read) -> io::Result<usize> {
            let mut buf = ReadBuf::new(&mut unsafe { self.buf.assume_init_mut() }[self.filled..]);
            let old_init = self.initialized - self.filled;
            unsafe {
                buf.assume_init(old_init);
            }
            reader.read_buf(buf.unfilled())?;
            self.filled += buf.len();
            self.initialized += buf.init_len() - old_init;
            Ok(buf.len())
        }

        /// Remove bytes that have already been read from the buffer.
        pub fn backshift(&mut self) {
            self.buf.copy_within(self.pos..self.filled, 0);
            self.filled -= self.pos;
            self.pos = 0;
        }

        #[inline]
        pub fn fill_buf(&mut self, mut reader: impl Read) -> io::Result<&[u8]> {
            // If we've reached the end of our internal buffer then we need to fetch
            // some more data from the reader.
            // Branch using `>=` instead of the more correct `==`
            // to tell the compiler that the pos..cap slice is always valid.
            if self.pos >= self.filled {
                debug_assert!(self.pos == self.filled);

                let mut buf = ReadBuf::new(unsafe { self.buf.assume_init_mut() });
                // SAFETY: `self.filled` bytes will always have been initialized.
                unsafe {
                    buf.assume_init(self.initialized);
                }

                let result = reader.read_buf(buf.unfilled());

                self.pos = 0;
                self.filled = buf.len();
                self.initialized = buf.init_len();

                result?;
            }
            Ok(self.buffer())
        }
    }

    // A wrapper around a byte buffer that is incrementally filled and initialized.
    ///
    /// This type is a sort of "double cursor". It tracks three regions in the
    /// buffer: a region at the beginning of the buffer that has been logically
    /// filled with data, a region that has been initialized at some point but not
    /// yet logically filled, and a region at the end that may be uninitialized.
    /// The filled region is guaranteed to be a subset of the initialized region.
    ///
    /// In summary, the contents of the buffer can be visualized as:
    ///
    /// ```not_rust
    /// [             capacity              ]
    /// [ filled |         unfilled         ]
    /// [    initialized    | uninitialized ]
    /// ```
    ///
    /// It is undefined behavior to de-initialize any bytes from the uninitialized
    /// region, since it is merely unknown whether this region is uninitialized or
    /// not, and if part of it turns out to be initialized, it must stay initialized.
    pub struct ReadBuf<'a> {
        buf: &'a mut [MaybeUninit<u8>],
        filled: usize,
        initialized: usize,
    }

    impl<'a> ReadBuf<'a> {
        /// Creates a new `ReadBuf` from a fully initialized buffer.
        #[inline]
        pub fn new(buf: &'a mut [u8]) -> ReadBuf<'a> {
            let initialized = buf.len();
            let buf = unsafe { slice_to_uninit_mut(buf) };
            ReadBuf {
                buf,
                filled: 0,
                initialized,
            }
        }

        /// Creates a new `ReadBuf` from a buffer that may be uninitialized.
        ///
        /// The internal cursor will mark the entire buffer as uninitialized. If
        /// the buffer is known to be partially initialized, then use `assume_init`
        /// to move the internal cursor.
        #[inline]
        pub fn uninit(buf: &'a mut [MaybeUninit<u8>]) -> ReadBuf<'a> {
            ReadBuf {
                buf,
                filled: 0,
                initialized: 0,
            }
        }

        #[inline]
        pub fn len(&self) -> usize {
            self.filled
        }

        /// Returns the length of the initialized part of the buffer.
        #[inline]
        pub fn init_len(&self) -> usize {
            self.initialized
        }

        /// Returns the total capacity of the buffer.
        #[inline]
        pub fn capacity(&self) -> usize {
            self.buf.len()
        }

        /// Returns a shared reference to the filled portion of the buffer.
        #[inline]
        pub fn filled(&self) -> &[u8] {
            let slice = &self.buf[..self.filled];
            // safety: filled describes how far into the buffer that the
            // user has filled with bytes, so it's been initialized.
            unsafe { slice_assume_init(slice) }
        }

        /// Returns a mutable reference to the filled portion of the buffer.
        #[inline]
        pub fn filled_mut(&mut self) -> &mut [u8] {
            let slice = &mut self.buf[..self.filled];
            // safety: filled describes how far into the buffer that the
            // user has filled with bytes, so it's been initialized.
            unsafe { slice_assume_init_mut(slice) }
        }

        /// Returns a new `ReadBuf` comprised of the unfilled section up to `n`.
        #[inline]
        pub fn take(&mut self, n: usize) -> ReadBuf<'_> {
            let max = std::cmp::min(self.remaining(), n);
            // Safety: We don't set any of the `unfilled_mut` with `MaybeUninit::uninit`.
            unsafe { ReadBuf::uninit(&mut self.unfilled_mut()[..max]) }
        }

        /// Returns a shared reference to the initialized portion of the buffer.
        ///
        /// This includes the filled portion.
        #[inline]
        pub fn initialized(&self) -> &[u8] {
            let slice = &self.buf[..self.initialized];
            // safety: initialized describes how far into the buffer that the
            // user has at some point initialized with bytes.
            unsafe { slice_assume_init(slice) }
        }

        /// Returns a mutable reference to the initialized portion of the buffer.
        ///
        /// This includes the filled portion.
        #[inline]
        pub fn initialized_mut(&mut self) -> &mut [u8] {
            let slice = &mut self.buf[..self.initialized];
            // safety: initialized describes how far into the buffer that the
            // user has at some point initialized with bytes.
            unsafe { slice_assume_init_mut(slice) }
        }

        /// Returns a mutable reference to the entire buffer, without ensuring that it has been fully
        /// initialized.
        ///
        /// The elements between 0 and `self.filled().len()` are filled, and those between 0 and
        /// `self.initialized().len()` are initialized (and so can be converted to a `&mut [u8]`).
        ///
        /// The caller of this method must ensure that these invariants are upheld. For example, if the
        /// caller initializes some of the uninitialized section of the buffer, it must call
        /// [`assume_init`](Self::assume_init) with the number of bytes initialized.
        ///
        /// # Safety
        ///
        /// The caller must not de-initialize portions of the buffer that have already been initialized.
        /// This includes any bytes in the region marked as uninitialized by `ReadBuf`.
        #[inline]
        pub unsafe fn inner_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            self.buf
        }

        /// Returns a mutable reference to the unfilled part of the buffer without ensuring that it has been fully
        /// initialized.
        ///
        /// # Safety
        ///
        /// The caller must not de-initialize portions of the buffer that have already been initialized.
        /// This includes any bytes in the region marked as uninitialized by `ReadBuf`.
        #[inline]
        pub unsafe fn unfilled_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            &mut self.buf[self.filled..]
        }

        /// Returns a mutable reference to the unfilled part of the buffer, ensuring it is fully initialized.
        ///
        /// Since `ReadBuf` tracks the region of the buffer that has been initialized, this is effectively "free" after
        /// the first use.
        #[inline]
        pub fn initialize_unfilled(&mut self) -> &mut [u8] {
            self.initialize_unfilled_to(self.remaining())
        }

        /// Returns a mutable reference to the first `n` bytes of the unfilled part of the buffer, ensuring it is
        /// fully initialized.
        ///
        /// # Panics
        ///
        /// Panics if `self.remaining()` is less than `n`.
        #[inline]
        #[track_caller]
        pub fn initialize_unfilled_to(&mut self, n: usize) -> &mut [u8] {
            assert!(self.remaining() >= n, "n overflows remaining");

            // This can't overflow, otherwise the assert above would have failed.
            let end = self.filled + n;

            if self.initialized < end {
                unsafe {
                    self.buf[self.initialized..end]
                        .as_mut_ptr()
                        .write_bytes(0, end - self.initialized);
                }
                self.initialized = end;
            }

            let slice = &mut self.buf[self.filled..end];
            // safety: just above, we checked that the end of the buf has
            // been initialized to some value.
            unsafe { slice_assume_init_mut(slice) }
        }

        /// Returns a cursor over the unfilled part of the buffer.
        #[inline]
        pub fn unfilled<'this>(&'this mut self) -> BorrowedCursor<'this> {
            BorrowedCursor {
                // SAFETY: we never assign into `BorrowedCursor::buf`, so treating its
                // lifetime covariantly is safe.
                buf: unsafe {
                    core::mem::transmute::<&'this mut ReadBuf<'a>, &'this mut ReadBuf<'this>>(self)
                },
            }
        }

        /// Returns the number of bytes at the end of the slice that have not yet been filled.
        #[inline]
        pub fn remaining(&self) -> usize {
            self.capacity() - self.filled
        }

        /// Clears the buffer, resetting the filled region to empty.
        ///
        /// The number of initialized bytes is not changed, and the contents of the buffer are not modified.
        #[inline]
        pub fn clear(&mut self) {
            self.filled = 0;
        }

        /// Advances the size of the filled region of the buffer.
        ///
        /// The number of initialized bytes is not changed.
        ///
        /// # Panics
        ///
        /// Panics if the filled region of the buffer would become larger than the initialized region.
        #[inline]
        #[track_caller]
        pub fn advance(&mut self, n: usize) {
            let new = self.filled.checked_add(n).expect("filled overflow");
            self.set_filled(new);
        }

        /// Sets the size of the filled region of the buffer.
        ///
        /// The number of initialized bytes is not changed.
        ///
        /// Note that this can be used to *shrink* the filled region of the buffer in addition to growing it (for
        /// example, by a `AsyncRead` implementation that compresses data in-place).
        ///
        /// # Panics
        ///
        /// Panics if the filled region of the buffer would become larger than the initialized region.
        #[inline]
        #[track_caller]
        pub fn set_filled(&mut self, n: usize) {
            assert!(
                n <= self.initialized,
                "filled must not become larger than initialized"
            );
            self.filled = n;
        }

        /// Asserts that the first `n` unfilled bytes of the buffer are initialized.
        ///
        /// `ReadBuf` assumes that bytes are never de-initialized, so this method does nothing when called with fewer
        /// bytes than are already known to be initialized.
        ///
        /// # Safety
        ///
        /// The caller must ensure that `n` unfilled bytes of the buffer have already been initialized.
        #[inline]
        pub unsafe fn assume_init(&mut self, n: usize) {
            let new = self.filled + n;
            if new > self.initialized {
                self.initialized = new;
            }
        }

        /// Appends data to the buffer, advancing the written position and possibly also the initialized position.
        ///
        /// # Panics
        ///
        /// Panics if `self.remaining()` is less than `buf.len()`.
        #[inline]
        #[track_caller]
        pub fn put_slice(&mut self, buf: &[u8]) {
            assert!(
                self.remaining() >= buf.len(),
                "buf.len() must fit in remaining(); buf.len() = {}, remaining() = {}",
                buf.len(),
                self.remaining()
            );

            let amt = buf.len();
            // Cannot overflow, asserted above
            let end = self.filled + amt;

            // Safety: the length is asserted above
            unsafe {
                self.buf[self.filled..end]
                    .as_mut_ptr()
                    .cast::<u8>()
                    .copy_from_nonoverlapping(buf.as_ptr(), amt);
            }

            if self.initialized < end {
                self.initialized = end;
            }
            self.filled = end;
        }
    }

    impl fmt::Debug for ReadBuf<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ReadBuf")
                .field("filled", &self.filled)
                .field("initialized", &self.initialized)
                .field("capacity", &self.capacity())
                .finish()
        }
    }

    /// # Safety
    ///
    /// The caller must ensure that `slice` is fully initialized
    /// and never writes uninitialized bytes to the returned slice.
    unsafe fn slice_to_uninit_mut(slice: &mut [u8]) -> &mut [MaybeUninit<u8>] {
        // SAFETY: `MaybeUninit<u8>` has the same memory layout as u8, and the caller
        // promises to not write uninitialized bytes to the returned slice.
        unsafe { &mut *(slice as *mut [u8] as *mut [MaybeUninit<u8>]) }
    }

    /// # Safety
    ///
    /// The caller must ensure that `slice` is fully initialized.
    // TODO: This could use `MaybeUninit::slice_assume_init` when it is stable.
    unsafe fn slice_assume_init(slice: &[MaybeUninit<u8>]) -> &[u8] {
        // SAFETY: `MaybeUninit<u8>` has the same memory layout as u8, and the caller
        // promises that `slice` is fully initialized.
        unsafe { &*(slice as *const [MaybeUninit<u8>] as *const [u8]) }
    }

    /// # Safety
    ///
    /// The caller must ensure that `slice` is fully initialized.
    // TODO: This could use `MaybeUninit::slice_assume_init_mut` when it is stable.
    unsafe fn slice_assume_init_mut(slice: &mut [MaybeUninit<u8>]) -> &mut [u8] {
        // SAFETY: `MaybeUninit<u8>` has the same memory layout as `u8`, and the caller
        // promises that `slice` is fully initialized.
        unsafe { &mut *(slice as *mut [MaybeUninit<u8>] as *mut [u8]) }
    }

    #[derive(Debug)]
    pub struct BorrowedCursor<'a> {
        /// The underlying buffer.
        // Safety invariant: we treat the type of buf as covariant in the lifetime of `BorrowedBuf` when
        // we create a `BorrowedCursor`. This is only safe if we never replace `buf` by assigning into
        // it, so don't do that!
        buf: &'a mut ReadBuf<'a>,
    }

    impl<'a> BorrowedCursor<'a> {
        /// Reborrows this cursor by cloning it with a smaller lifetime.
        ///
        /// Since a cursor maintains unique access to its underlying buffer, the borrowed cursor is
        /// not accessible while the new cursor exists.
        #[inline]
        pub fn reborrow<'this>(&'this mut self) -> BorrowedCursor<'this> {
            BorrowedCursor {
                // SAFETY: we never assign into `BorrowedCursor::buf`, so treating its
                // lifetime covariantly is safe.
                buf: unsafe {
                    core::mem::transmute::<&'this mut ReadBuf<'a>, &'this mut ReadBuf<'this>>(
                        self.buf,
                    )
                },
            }
        }

        /// Returns the available space in the cursor.
        #[inline]
        pub fn capacity(&self) -> usize {
            self.buf.capacity() - self.buf.filled
        }

        /// Returns the number of bytes written to the `BorrowedBuf` this cursor was created from.
        ///
        /// In particular, the count returned is shared by all reborrows of the cursor.
        #[inline]
        pub fn written(&self) -> usize {
            self.buf.filled
        }

        /// Returns a mutable reference to the initialized portion of the cursor.
        #[inline]
        pub fn init_mut(&mut self) -> &mut [u8] {
            // SAFETY: We only slice the initialized part of the buffer, which is always valid
            unsafe {
                let buf = self
                    .buf
                    .buf
                    .get_unchecked_mut(self.buf.filled..self.buf.initialized);
                buf.assume_init_mut()
            }
        }

        /// Returns a mutable reference to the whole cursor.
        ///
        /// # Safety
        ///
        /// The caller must not uninitialize any bytes in the initialized portion of the cursor.
        #[inline]
        pub unsafe fn as_mut(&mut self) -> &mut [MaybeUninit<u8>] {
            // SAFETY: always in bounds
            unsafe { self.buf.buf.get_unchecked_mut(self.buf.filled..) }
        }

        /// Advances the cursor by asserting that `n` bytes have been filled.
        ///
        /// After advancing, the `n` bytes are no longer accessible via the cursor and can only be
        /// accessed via the underlying buffer. I.e., the buffer's filled portion grows by `n` elements
        /// and its unfilled portion (and the capacity of this cursor) shrinks by `n` elements.
        ///
        /// If less than `n` bytes initialized (by the cursor's point of view), `set_init` should be
        /// called first.
        ///
        /// # Panics
        ///
        /// Panics if there are less than `n` bytes initialized.
        #[inline]
        pub fn advance(&mut self, n: usize) -> &mut Self {
            // The subtraction cannot underflow by invariant of this type.
            assert!(n <= self.buf.initialized - self.buf.filled);

            self.buf.filled += n;
            self
        }

        /// Advances the cursor by asserting that `n` bytes have been filled.
        ///
        /// After advancing, the `n` bytes are no longer accessible via the cursor and can only be
        /// accessed via the underlying buffer. I.e., the buffer's filled portion grows by `n` elements
        /// and its unfilled portion (and the capacity of this cursor) shrinks by `n` elements.
        ///
        /// # Safety
        ///
        /// The caller must ensure that the first `n` bytes of the cursor have been properly
        /// initialised.
        #[inline]
        pub unsafe fn advance_unchecked(&mut self, n: usize) -> &mut Self {
            self.buf.filled += n;
            self.buf.initialized = cmp::max(self.buf.initialized, self.buf.filled);
            self
        }

        /// Initializes all bytes in the cursor.
        #[inline]
        pub fn ensure_init(&mut self) -> &mut Self {
            // SAFETY: always in bounds and we never uninitialize these bytes.
            let uninit = unsafe { self.buf.buf.get_unchecked_mut(self.buf.initialized..) };

            // SAFETY: 0 is a valid value for MaybeUninit<u8> and the length matches the allocation
            // since it is comes from a slice reference.
            unsafe {
                core::ptr::write_bytes(uninit.as_mut_ptr(), 0, uninit.len());
            }
            self.buf.initialized = self.buf.capacity();

            self
        }

        /// Asserts that the first `n` unfilled bytes of the cursor are initialized.
        ///
        /// `BorrowedBuf` assumes that bytes are never de-initialized, so this method does nothing when
        /// called with fewer bytes than are already known to be initialized.
        ///
        /// # Safety
        ///
        /// The caller must ensure that the first `n` bytes of the buffer have already been initialized.
        #[inline]
        pub unsafe fn set_init(&mut self, n: usize) -> &mut Self {
            self.buf.initialized = cmp::max(self.buf.initialized, self.buf.filled + n);
            self
        }

        /// Appends data to the cursor, advancing position within its buffer.
        ///
        /// # Panics
        ///
        /// Panics if `self.capacity()` is less than `buf.len()`.
        #[inline]
        pub fn append(&mut self, buf: &[u8]) {
            assert!(self.capacity() >= buf.len());

            // SAFETY: we do not de-initialize any of the elements of the slice
            unsafe {
                self.as_mut()[..buf.len()].write_copy_of_slice(buf);
            }

            // SAFETY: We just added the entire contents of buf to the filled section.
            unsafe {
                self.set_init(buf.len());
            }
            self.buf.filled += buf.len();
        }

        /// Runs the given closure with a `BorrowedBuf` containing the unfilled part
        /// of the cursor.
        ///
        /// This enables inspecting what was written to the cursor.
        ///
        /// # Panics
        ///
        /// Panics if the `BorrowedBuf` given to the closure is replaced by another
        /// one.
        pub fn with_unfilled_buf<T>(&mut self, f: impl FnOnce(&mut ReadBuf<'_>) -> T) -> T {
            let mut buf = ReadBuf::from(self.reborrow());
            let prev_ptr = buf.buf as *const _;
            let res = f(&mut buf);

            // Check that the caller didn't replace the `BorrowedBuf`.
            // This is necessary for the safety of the code below: if the check wasn't
            // there, one could mark some bytes as initialized even though there aren't.
            assert!(core::ptr::addr_eq(prev_ptr, buf.buf));

            let filled = buf.filled;
            let init = buf.initialized;

            // Update `init` and `filled` fields with what was written to the buffer.
            // `self.buf.filled` was the starting length of the `BorrowedBuf`.
            //
            // SAFETY: These amounts of bytes were initialized/filled in the `BorrowedBuf`,
            // and therefore they are initialized/filled in the cursor too, because the
            // buffer wasn't replaced.
            self.buf.initialized = self.buf.filled + init;
            self.buf.filled += filled;

            res
        }
    }

    impl<'data> From<BorrowedCursor<'data>> for ReadBuf<'data> {
        #[inline]
        fn from(mut buf: BorrowedCursor<'data>) -> ReadBuf<'data> {
            let initialized = buf.init_mut().len();
            ReadBuf {
                // SAFETY: no initialized byte is ever uninitialized as per
                // `BorrowedBuf`'s invariant
                buf: unsafe { buf.buf.buf.get_unchecked_mut(buf.buf.filled..) },
                filled: 0,
                initialized,
            }
        }
    }
}
