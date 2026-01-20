use core::fmt;

use crate::StringMsg;
/// A specialized [`Result`] type for I/O operations.
///
/// This type is broadly used across [`mycelium_util::io`] for any operation which may
/// produce an error.
///
/// This typedef is generally used to avoid writing out [`io::Error`] directly and
/// is otherwise a direct mapping to [`Result`].
///
/// While usual Rust style is to import types directly, aliases of [`Result`]
/// often are not, to make it easier to distinguish between them. [`Result`] is
/// generally assumed to be [`core::result::Result`][`Result`], and so users of this alias
/// will generally use `io::Result` instead of shadowing the [prelude]'s import
/// of [`core::result::Result`][`Result`].
///
/// [`mycelium_util::io`]: crate::io
/// [`io::Error`]: Error
/// [`Result`]: core::result::Result
/// [prelude]: core::prelude
pub type Result<T> = core::result::Result<T, self::Error>;

/// The error type for I/O operations of the [`Read`], [`Write`], [`Seek`], and
/// associated traits.
///
/// Errors mostly originate from the underlying OS, but custom instances of
/// `Error` can be created with crafted error messages and a particular value of
/// [`ErrorKind`].
///
/// [`Read`]: crate::io::Read
/// [`Write`]: crate::io::Write
/// [`Seek`]: crate::io::Seek
#[derive(Debug)]
pub struct Error<E: core::error::Error + 'static = StringMsg> {
    kind: ErrorKind,
    source: Option<E>,
}

/// A list specifying general categories of I/O error.
///
/// This list is intended to grow over time and it is not recommended to
/// exhaustively match against it.
///
/// It is used with the [`io::Error`] type.
///
/// [`io::Error`]: Error
///
/// # Handling errors and matching on `ErrorKind`
///
/// In application code, use `match` for the `ErrorKind` values you are
/// expecting; use `_` to match "all other errors".
///
/// In comprehensive and thorough tests that want to verify that a test doesn't
/// return any known incorrect error kind, you may want to cut-and-paste the
/// current full list of errors from here into your test code, and then match
/// `_` as the correct case. This seems counterintuitive, but it will make your
/// tests more robust. In particular, if you want to verify that your code does
/// produce an unrecognized error kind, the robust solution is to check for all
/// the recognized error kinds and fail in those cases.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ErrorKind {
    /// An entity was not found, often a file.
    NotFound,
    /// The operation lacked the necessary privileges to complete.
    PermissionDenied,
    /// The connection was refused by the remote server.
    ConnectionRefused,
    /// The connection was reset by the remote server.
    ConnectionReset,
    /// The connection was aborted (terminated) by the remote server.
    ConnectionAborted,
    /// The network operation failed because it was not connected yet.
    NotConnected,
    /// A socket address could not be bound because the address is already in
    /// use elsewhere.
    AddrInUse,
    /// A nonexistent interface was requested or the requested address was not
    /// local.
    AddrNotAvailable,
    /// The operation failed because a pipe was closed.
    BrokenPipe,
    /// An entity already exists, often a file.
    AlreadyExists,
    /// The operation needs to block to complete, but the blocking operation was
    /// requested to not occur.
    WouldBlock,
    /// A parameter was incorrect.
    InvalidInput,
    /// Data not valid for the operation were encountered.
    ///
    /// Unlike [`InvalidInput`], this typically means that the operation
    /// parameters were valid, however the error was caused by malformed
    /// input data.
    ///
    /// For example, a function that reads a file into a string will error with
    /// `InvalidData` if the file's contents are not valid UTF-8.
    ///
    /// [`InvalidInput`]: #variant.InvalidInput
    InvalidData,
    /// The I/O operation's timeout expired, causing it to be canceled.
    TimedOut,
    /// An error returned when an operation could not be completed because a
    /// call to [`write`] returned [`Ok(0)`].
    ///
    /// This typically means that an operation could only succeed if it wrote a
    /// particular number of bytes but only a smaller number of bytes could be
    /// written.
    ///
    /// [`write`]: ../../std/io/trait.Write.html#tymethod.write
    /// [`Ok(0)`]: ../../std/io/type.Result.html
    WriteZero,
    /// This operation was interrupted.
    ///
    /// Interrupted operations can typically be retried.
    Interrupted,
    /// Any I/O error not part of this list.
    Other,

    /// An error returned when an operation could not be completed because an
    /// "end of file" was reached prematurely.
    ///
    /// This typically means that an operation could only succeed if it read a
    /// particular number of bytes but only a smaller number of bytes could be
    /// read.
    UnexpectedEof,
}

impl<E: core::error::Error + 'static> Error<E> {
    /// Returns a new I/O error with the provided [`ErrorKind`] and `source`
    /// error.
    #[must_use]
    #[inline]
    pub fn new(kind: ErrorKind, source: E) -> Self {
        Self {
            kind,
            source: Some(source),
        }
    }

    /// Returns the [`ErrorKind`] of this error.
    #[must_use]
    #[inline]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Error { kind, source: None }
    }
}

impl<E: core::error::Error + 'static> core::error::Error for Error<E> {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e as &(dyn core::error::Error + 'static))
    }
}

impl<E: core::error::Error + 'static> fmt::Display for Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref source) = self.source {
            write!(f, "{}: {}", self.kind.as_str(), source)
        } else {
            f.write_str(self.kind.as_str())
        }
    }
}

impl ErrorKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            ErrorKind::NotFound => "entity not found",
            ErrorKind::PermissionDenied => "permission denied",
            ErrorKind::ConnectionRefused => "connection refused",
            ErrorKind::ConnectionReset => "connection reset",
            ErrorKind::ConnectionAborted => "connection aborted",
            ErrorKind::NotConnected => "not connected",
            ErrorKind::AddrInUse => "address in use",
            ErrorKind::AddrNotAvailable => "address not available",
            ErrorKind::BrokenPipe => "broken pipe",
            ErrorKind::AlreadyExists => "entity already exists",
            ErrorKind::WouldBlock => "operation would block",
            ErrorKind::InvalidInput => "invalid input parameter",
            ErrorKind::InvalidData => "invalid data",
            ErrorKind::TimedOut => "timed out",
            ErrorKind::WriteZero => "write zero",
            ErrorKind::Interrupted => "operation interrupted",
            ErrorKind::Other => "other error",
            ErrorKind::UnexpectedEof => "unexpected end of file",
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::ErrorKind> for ErrorKind {
    fn from(value: std::io::ErrorKind) -> Self {
        match value {
            std::io::ErrorKind::ConnectionRefused => ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionReset => ErrorKind::ConnectionReset,
            std::io::ErrorKind::ConnectionAborted => ErrorKind::ConnectionAborted,
            std::io::ErrorKind::NotConnected => ErrorKind::NotConnected,
            std::io::ErrorKind::AddrInUse => ErrorKind::AddrInUse,
            std::io::ErrorKind::AddrNotAvailable => ErrorKind::AddrNotAvailable,
            std::io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
            std::io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            std::io::ErrorKind::WouldBlock => ErrorKind::WouldBlock,
            std::io::ErrorKind::InvalidInput => ErrorKind::InvalidInput,
            std::io::ErrorKind::InvalidData => ErrorKind::InvalidData,
            std::io::ErrorKind::TimedOut => ErrorKind::TimedOut,
            std::io::ErrorKind::WriteZero => ErrorKind::WriteZero,
            std::io::ErrorKind::Interrupted => ErrorKind::Interrupted,
            std::io::ErrorKind::Other => ErrorKind::Other,
            std::io::ErrorKind::UnexpectedEof => ErrorKind::UnexpectedEof,
            _ => ErrorKind::Other,
        }
    }
}

impl From<embedded_io::ErrorKind> for ErrorKind {
    fn from(value: embedded_io::ErrorKind) -> Self {
        match value {
            embedded_io::ErrorKind::ConnectionRefused => ErrorKind::ConnectionRefused,
            embedded_io::ErrorKind::ConnectionReset => ErrorKind::ConnectionReset,
            embedded_io::ErrorKind::ConnectionAborted => ErrorKind::ConnectionAborted,
            embedded_io::ErrorKind::NotConnected => ErrorKind::NotConnected,
            embedded_io::ErrorKind::AddrInUse => ErrorKind::AddrInUse,
            embedded_io::ErrorKind::AddrNotAvailable => ErrorKind::AddrNotAvailable,
            embedded_io::ErrorKind::BrokenPipe => ErrorKind::BrokenPipe,
            embedded_io::ErrorKind::AlreadyExists => ErrorKind::AlreadyExists,
            embedded_io::ErrorKind::InvalidInput => ErrorKind::InvalidInput,
            embedded_io::ErrorKind::InvalidData => ErrorKind::InvalidData,
            embedded_io::ErrorKind::TimedOut => ErrorKind::TimedOut,
            embedded_io::ErrorKind::WriteZero => ErrorKind::WriteZero,
            embedded_io::ErrorKind::Interrupted => ErrorKind::Interrupted,
            embedded_io::ErrorKind::Other => ErrorKind::Other,
            embedded_io::ErrorKind::NotFound => ErrorKind::NotFound,
            embedded_io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            embedded_io::ErrorKind::Unsupported => ErrorKind::Other,
            embedded_io::ErrorKind::OutOfMemory => ErrorKind::Other,
            _ => ErrorKind::Other,
        }
    }
}

impl From<ErrorKind> for embedded_io::ErrorKind {
    fn from(value: ErrorKind) -> Self {
        match value {
            ErrorKind::ConnectionRefused => embedded_io::ErrorKind::ConnectionRefused,
            ErrorKind::ConnectionReset => embedded_io::ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted => embedded_io::ErrorKind::ConnectionAborted,
            ErrorKind::NotConnected => embedded_io::ErrorKind::NotConnected,
            ErrorKind::AddrInUse => embedded_io::ErrorKind::AddrInUse,
            ErrorKind::AddrNotAvailable => embedded_io::ErrorKind::AddrNotAvailable,
            ErrorKind::BrokenPipe => embedded_io::ErrorKind::BrokenPipe,
            ErrorKind::AlreadyExists => embedded_io::ErrorKind::AlreadyExists,
            ErrorKind::InvalidInput => embedded_io::ErrorKind::InvalidInput,
            ErrorKind::InvalidData => embedded_io::ErrorKind::InvalidData,
            ErrorKind::TimedOut => embedded_io::ErrorKind::TimedOut,
            ErrorKind::WriteZero => embedded_io::ErrorKind::WriteZero,
            ErrorKind::Interrupted => embedded_io::ErrorKind::Interrupted,
            ErrorKind::Other => embedded_io::ErrorKind::Other,
            ErrorKind::NotFound => embedded_io::ErrorKind::NotFound,
            ErrorKind::PermissionDenied => embedded_io::ErrorKind::PermissionDenied,
            ErrorKind::WouldBlock => embedded_io::ErrorKind::TimedOut,
            ErrorKind::UnexpectedEof => embedded_io::ErrorKind::BrokenPipe,
        }
    }
}

#[cfg(feature = "std")]
impl From<ErrorKind> for std::io::ErrorKind {
    fn from(value: ErrorKind) -> Self {
        match value {
            ErrorKind::AlreadyExists => std::io::ErrorKind::AlreadyExists,
            ErrorKind::InvalidInput => std::io::ErrorKind::InvalidInput,
            ErrorKind::InvalidData => std::io::ErrorKind::InvalidData,
            ErrorKind::TimedOut => std::io::ErrorKind::TimedOut,
            ErrorKind::WriteZero => std::io::ErrorKind::WriteZero,
            ErrorKind::Interrupted => std::io::ErrorKind::Interrupted,
            ErrorKind::Other => std::io::ErrorKind::Other,
            ErrorKind::NotFound => std::io::ErrorKind::NotFound,
            ErrorKind::PermissionDenied => std::io::ErrorKind::PermissionDenied,
            ErrorKind::WouldBlock => std::io::ErrorKind::WouldBlock,
            ErrorKind::UnexpectedEof => std::io::ErrorKind::UnexpectedEof,
            ErrorKind::BrokenPipe => std::io::ErrorKind::BrokenPipe,
            ErrorKind::ConnectionReset => std::io::ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted => std::io::ErrorKind::ConnectionAborted,
            ErrorKind::NotConnected => std::io::ErrorKind::NotConnected,
            ErrorKind::ConnectionRefused => std::io::ErrorKind::ConnectionRefused,
            ErrorKind::AddrInUse => std::io::ErrorKind::AddrInUse,
            ErrorKind::AddrNotAvailable => std::io::ErrorKind::AddrNotAvailable,
        }
    }
}

#[cfg(feature = "std")]
impl From<Error> for std::io::Error {
    fn from(value: Error) -> Self {
        std::io::Error::new(value.kind().into(), value)
    }
}
