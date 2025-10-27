//! Implementation functions for derive macros
//!
//! These functions are used by the `jacquard-derive` proc-macro crate but are also
//! available for runtime code generation in `jacquard-lexicon`.

pub mod doc_to_tokens;
pub mod helpers;
pub mod into_static;
pub mod lexicon_attr;
pub mod lexicon_schema;
pub mod lexicon_union;
pub mod open_union_attr;
pub mod xrpc_request;

// Re-export the main entry points
pub use into_static::impl_derive_into_static;
pub use lexicon_attr::impl_lexicon;
pub use lexicon_schema::impl_derive_lexicon_schema;
pub use lexicon_union::impl_lexicon_union;
pub use open_union_attr::impl_open_union;
pub use xrpc_request::impl_derive_xrpc_request;
