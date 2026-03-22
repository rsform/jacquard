//! Borrow-or-share traits for abstracting over owned and borrowed string representations.
//!
//! This module is a vendored copy of the [`borrow-or-share`](https://docs.rs/borrow-or-share/0.2.4/)
//! crate by yescallop, with additional implementations for [`SmolStr`](smol_str::SmolStr)
//! and [`CowStr`](crate::CowStr). We vendor rather than depend on the crate to avoid
//! orphan rule issues. We need to implement `Bos<str>` for `SmolStr` and `CowStr`, which
//! are foreign types relative to the upstream crate.
//!
//! # Overview
//!
//! [`Bos<T>`] is the base trait providing a GAT for the reference type. Use it as a bound
//! when you need a method that borrows from `*self` regardless of whether the backing type
//! is owned or borrowed:
//!
//! ```ignore
//! impl<T: Bos<str>> AsRef<str> for MyType<T> {
//!     fn as_ref(&self) -> &str {
//!         self.as_str()
//!     }
//! }
//! ```
//!
//! [`BorrowOrShare<'i, 'o, T>`] is the convenience trait with split lifetimes. Use it when
//! you want a method on `&'i self` that returns `&'o T`, where `'o` may outlive `'i` when
//! the backing type is a reference:
//!
//! ```ignore
//! impl<'i, 'o, T: BorrowOrShare<'i, 'o, str>> MyType<T> {
//!     fn as_str(&'i self) -> &'o str {
//!         self.0.borrow_or_share()
//!     }
//! }
//! ```

use alloc::{
    borrow::{Cow, ToOwned},
    boxed::Box,
    string::String,
    vec::Vec,
};

use smol_str::SmolStr;

use crate::CowStr;

mod internal {
    pub trait Ref<T: ?Sized> {
        fn cast<'a>(self) -> &'a T
        where
            Self: 'a;
    }

    impl<T: ?Sized> Ref<T> for &T {
        #[inline]
        fn cast<'a>(self) -> &'a T
        where
            Self: 'a,
        {
            self
        }
    }
}

use internal::Ref;

/// A trait for either borrowing or sharing data.
///
/// See the [module-level documentation](self) for more details.
pub trait Bos<T: ?Sized> {
    /// The resulting reference type. May only be `&T`.
    type Ref<'this>: Ref<T>
    where
        Self: 'this;

    /// Borrows from `*this` or from behind a reference it holds,
    /// returning a reference of type [`Self::Ref`].
    ///
    /// In the latter case, the returned reference is said to be *shared* with `*this`.
    fn borrow_or_share(this: &Self) -> Self::Ref<'_>;
}

/// A helper trait for writing "data borrowing or sharing" functions.
///
/// See the [module-level documentation](self) for more details.
pub trait BorrowOrShare<'i, 'o, T: ?Sized>: Bos<T> {
    /// Borrows from `*self` or from behind a reference it holds.
    ///
    /// In the latter case, the returned reference is said to be *shared* with `*self`.
    fn borrow_or_share(&'i self) -> &'o T;
}

impl<'i, 'o, T: ?Sized, B> BorrowOrShare<'i, 'o, T> for B
where
    B: Bos<T> + ?Sized + 'i,
    B::Ref<'i>: 'o,
{
    #[inline]
    fn borrow_or_share(&'i self) -> &'o T {
        (B::borrow_or_share(self) as B::Ref<'i>).cast()
    }
}

// --- Reference impl (sharing) ---

impl<'a, T: ?Sized> Bos<T> for &'a T {
    type Ref<'this>
        = &'a T
    where
        Self: 'this;

    #[inline]
    fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
        this
    }
}

// --- Macro for borrowing impls ---

/// Implement [`Bos`] for types that always borrow from `*self`.
///
/// Each entry maps a concrete type to the target slice/str type it derefs to.
/// The generated impl uses `Ref<'this> = &'this $target` — pure borrowing, no sharing.
#[macro_export]
macro_rules! impl_bos {
    ($($(#[$attr:meta])? $({$($params:tt)*})? $ty:ty => $target:ty)*) => {
        $(
            $(#[$attr])?
            impl $(<$($params)*>)? $crate::bos::Bos<$target> for $ty {
                type Ref<'this> = &'this $target where Self: 'this;

                #[inline]
                fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
                    this
                }
            }
        )*
    };
}

// --- Standard library impls ---

impl_bos! {
    //{T: ?Sized} T => T

    {T: ?Sized} &mut T => T

    {T, const N: usize} [T; N] => [T]

    {T} Vec<T> => [T]

    String => str

    {T: ?Sized} Box<T> => T
    {B: ?Sized + ToOwned} Cow<'_, B> => B

    {T: ?Sized} alloc::sync::Arc<T> => T
    {T: ?Sized} alloc::rc::Rc<T> => T
}

#[cfg(feature = "std")]
impl_bos! {
    std::ffi::OsString => std::ffi::OsStr
    std::path::PathBuf => std::path::Path
    alloc::ffi::CString => core::ffi::CStr
}

// --- SmolStr impl ---

impl_bos! {
    SmolStr => str
}

// --- CowStr impl ---

impl<'a> Bos<str> for CowStr<'a> {
    type Ref<'this>
        = &'this str
    where
        Self: 'this;

    #[inline]
    fn borrow_or_share(this: &Self) -> Self::Ref<'_> {
        this.as_str()
    }
}

/// The default string backing type for jacquard's type-parameterised types.
///
/// `SmolStr` is used as the default because it satisfies `DeserializeOwned` (no lifetime
/// annotation required) and provides small-string inline storage without heap allocation
/// for strings of 22 bytes or fewer.
pub type DefaultStr = SmolStr;

#[cfg(test)]
mod tests {
    use super::*;

    // Verify BorrowOrShare works for all backing types.

    fn as_str_via_bos<'i, 'o, S: BorrowOrShare<'i, 'o, str>>(s: &'i S) -> &'o str {
        s.borrow_or_share()
    }

    #[test]
    fn bos_smolstr() {
        let s = SmolStr::new("hello");
        assert_eq!(as_str_via_bos(&s), "hello");
    }

    #[test]
    fn bos_string() {
        let s = String::from("hello");
        assert_eq!(as_str_via_bos(&s), "hello");
    }

    #[test]
    fn bos_ref_str() {
        let s: &str = "hello";
        assert_eq!(as_str_via_bos(&s), "hello");
    }

    #[test]
    fn bos_cowstr_borrowed() {
        let s = CowStr::Borrowed("hello");
        assert_eq!(as_str_via_bos(&s), "hello");
    }

    #[test]
    fn bos_cowstr_owned() {
        let s = CowStr::Owned(SmolStr::new("hello"));
        assert_eq!(as_str_via_bos(&s), "hello");
    }

    // Verify Bos (non-sharing) works via AsRef-style usage.

    fn as_ref_via_bos<S: Bos<str>>(s: &S) -> &str {
        let r = S::borrow_or_share(s);
        r.cast()
    }

    #[test]
    fn bos_as_ref_smolstr() {
        let s = SmolStr::new("world");
        assert_eq!(as_ref_via_bos(&s), "world");
    }

    #[test]
    fn bos_as_ref_ref_str() {
        let s: &str = "world";
        assert_eq!(as_ref_via_bos(&s), "world");
    }

    // Verify sharing semantics: &str reference outlives the wrapper.

    #[test]
    fn ref_str_sharing_outlives_wrapper() {
        let original: &str = "shared";
        let result: &str;
        {
            let wrapper: &&str = &original;
            result = as_str_via_bos(wrapper);
        }
        // result outlives wrapper because &str shares, not borrows.
        assert_eq!(result, "shared");
    }
}
