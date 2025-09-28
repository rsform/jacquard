pub mod aturi;
pub mod blob;
pub mod cid;
pub mod collection;
pub mod datetime;
pub mod did;
pub mod handle;
pub mod ident;
pub mod integer;
pub mod language;
pub mod link;
pub mod nsid;
pub mod recordkey;
pub mod tid;

/// Trait for a constant string literal type
pub trait Literal: Clone + Copy + PartialEq + Eq + Send + Sync + 'static {
    /// The string literal
    const LITERAL: &'static str;
}
