#![feature(maybe_uninit_slice)]
#![feature(maybe_uninit_write_slice)]
#![cfg_attr(target_os = "none", no_std)]

#[cfg(all(not(feature = "std"), feature = "alloc"))]
extern crate alloc;

#[cfg(all(feature = "std", feature = "alloc"))]
use std as alloc;

/// Lazy initialization type for static values.
///
/// Uses `std::sync::LazyLock` when the `std` feature is enabled, or `spin::Lazy`
/// for no_std environments. Exported so downstream crates can use it without
/// their own conditional compilation.
#[cfg(feature = "std")]
pub type Lazy<T> = std::sync::LazyLock<T>;

#[cfg(not(feature = "std"))]
pub use spin::Lazy;

pub mod deferred;
pub mod io;
//pub mod map;
pub mod storage;
pub mod vector;

use core::ops::{Deref, DerefMut};

pub struct ScopeGuard<'a, T>(pub &'a mut T, pub fn(&mut T));

impl<T> Deref for ScopeGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<T> DerefMut for ScopeGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<T> Drop for ScopeGuard<'_, T> {
    #[inline]
    fn drop(&mut self) {
        (self.1)(self.0);
    }
}

#[derive(Debug)]
pub struct StringMsg(&'static str);

impl From<&'static str> for StringMsg {
    fn from(s: &'static str) -> Self {
        StringMsg(s)
    }
}

impl core::fmt::Display for StringMsg {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl core::error::Error for StringMsg {}

use core::num::Wrapping;

#[derive(Clone, Copy, PartialEq, Eq)]
/// Unique ID used to identify things in the open Volume/File/Directory lists
pub struct Handle(pub(crate) u32);

impl core::fmt::Debug for Handle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#08x}", self.0)
    }
}

/// A Handle Generator.
///
/// This object will always return a different ID.
///
/// Well, it will wrap after `2**32` IDs. But most systems won't open that many
/// files, and if they do, they are unlikely to hold one file open and then
/// open/close `2**32 - 1` others.
#[derive(Debug)]
pub struct HandleGenerator {
    next_id: Wrapping<u32>,
}

impl HandleGenerator {
    /// Create a new generator of Handles.
    pub const fn new(offset: u32) -> Self {
        Self {
            next_id: Wrapping(offset),
        }
    }

    /// Generate a new, unique [`Handle`].
    pub fn generate(&mut self) -> Handle {
        let id = self.next_id;
        self.next_id += 1;
        Handle(id.0)
    }
}

static HANDLE_GENERATOR: Lazy<HandleGenerator> = Lazy::new(|| HandleGenerator::new(0));

pub fn new_handle() -> Handle {
    // TODO: Fix this for multithreaded systems using a thread-local and a unique id or something
    todo!()
}

#[cfg(test)]
mod tests {}
