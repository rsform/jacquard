//! # Common types for the jacquard implementation of atproto
//!
//! ## Working with Lifetimes and Zero-Copy Deserialization
//!
//! Jacquard is designed around zero-copy deserialization: types like `Post<'de>` can borrow
//! strings and other data directly from the response buffer instead of allocating owned copies.
//! This is great for performance, but it creates some interesting challenges when combined with
//! async Rust and trait bounds.
//!
//! ### The Problem: Lifetimes + Async + Traits
//!
//! The naive approach would be to put a lifetime parameter on the trait itself:
//!
//! ```ignore
//! trait XrpcRequest<'de> {
//!     type Output: Deserialize<'de>;
//!     // ...
//! }
//! ```
//!
//! This looks reasonable until you try to use it in a generic context. If you have a function
//! that works with *any* lifetime, you need a Higher-Ranked Trait Bound (HRTB):
//!
//! ```ignore
//! fn foo<R>(response: &[u8])
//! where
//!     R: for<'any> XrpcRequest<'any>
//! {
//!     // deserialize from response...
//! }
//! ```
//!
//! The `for<'any>` bound says "this type must implement `XrpcRequest` for *every possible lifetime*",
//! which is effectively the same as requiring `DeserializeOwned`. You've just thrown away your
//! zero-copy optimization, and this also won't work on most of the types in jacquard. The vast
//! majority of them have either a custom Deserialize implementation which will borrow if it
//! can, a #[serde(borrow)] attribute on one or more fields, or an equivalent lifetime bound
//! attribute, associated with the Deserialize derive macro.
//!
//! It gets worse with async. If you want to return borrowed data from an async method, where does
//! the lifetime come from? The response buffer needs to outlive the borrow, but the buffer is
//! consumed by the HTTP call. You end up with "cannot infer appropriate lifetime" errors or even
//! more confusing errors because the compiler can't prove the buffer will stay alive. You *could*
//! do some lifetime laundering with `unsafe`, but you don't actually *need* to tell rustc to "trust
//! me, bro", you can, with some cleverness, explain this to the compiler in a way that it can
//! reason about perfectly well.
//!
//! ### Explaining where the buffer goes to `rustc`: GATs + Method-Level Lifetimes
//!
//! The fix is to use Generic Associated Types (GATs) on the trait's associated types, while keeping
//! the trait itself lifetime-free:
//!
//! ```ignore
//! trait XrpcResp {
//!     const NSID: &'static str;
//!
//!     // GATs: lifetime is on the associated type, not the trait
//!     type Output<'de>: Deserialize<'de> + IntoStatic;
//!     type Err<'de>: Deserialize<'de> + IntoStatic;
//! }
//! ```
//!
//! Now you can write trait bounds without HRTBs:
//!
//! ```ignore
//! fn foo<R: XrpcResp>(response: &[u8]) {
//!     // Compiler can pick a concrete lifetime for R::Output<'_>
//! }
//! ```
//!
//! Methods that need lifetimes use method-level generic parameters:
//!
//! ```ignore
//! // This is part of a trait from jacquard itself, used to genericize updates to the Bluesky
//! // preferences union, so that if you implement a similar lexicon type in your AppView or App
//! // Server API, you don't have to special-case it.
//!
//! trait VecUpdate {
//!     type GetRequest<'de>: XrpcRequest<'de>;  // GAT
//!     type PutRequest<'de>: XrpcRequest<'de>;  // GAT
//!
//!     // Method-level lifetime, not trait-level
//!     fn extract_vec<'s>(
//!         output: <Self::GetRequest<'s> as XrpcRequest<'s>>::Output<'s>
//!     ) -> Vec<Self::Item>;
//! }
//! ```
//!
//! The compiler can monomorphize for concrete lifetimes instead of trying to prove bounds hold
//! for *all* lifetimes at once.
//!
//! ### Handling Async with `Response<R: XrpcResp>`
//!
//! For the async problem, we use a wrapper type that owns the response buffer:
//!
//! ```ignore
//! pub struct Response<R: XrpcResp> {
//!     buffer: Bytes,  // Refcounted, cheap to clone
//!     status: StatusCode,
//!     _marker: PhantomData<R>,
//! }
//! ```
//!
//! This lets async methods return a `Response` that owns its buffer, then the *caller* decides
//! the lifetime strategy:
//!
//! ```ignore
//! // Zero-copy: borrow from the owned buffer
//! let output: R::Output<'_> = response.parse()?;
//!
//! // Owned: convert to 'static via IntoStatic
//! let output: R::Output<'static> = response.into_output()?;
//! ```
//!
//! The async method doesn't need to know or care about lifetimes - it just returns the `Response`.
//! The caller gets full control over whether to use borrowed or owned data. It can even decide
//! after the fact that it doesn't want to parse out the API response type that it asked for. Instead
//! it can call `.parse_data()` or `.parse_raw()` on the response to get loosely typed, validated
//! data or minimally typed maximally accepting data values out.
//!
//! ### Example: XRPC Traits in Practice
//!
//! Here's how the pattern works with the XRPC layer:
//!
//! ```ignore
//! // XrpcResp uses GATs, not trait-level lifetime
//! trait XrpcResp {
//!     const NSID: &'static str;
//!     type Output<'de>: Deserialize<'de> + IntoStatic;
//!     type Err<'de>: Deserialize<'de> + IntoStatic;
//! }
//!
//! // Response owns the buffer (Bytes is refcounted)
//! pub struct Response<R: XrpcResp> {
//!     buffer: Bytes,
//!     status: StatusCode,
//!     _marker: PhantomData<R>,
//! }
//!
//! impl<R: XrpcResp> Response<R> {
//!     // Borrow from owned buffer
//!     pub fn parse(&self) -> XrpcResult<R::Output<'_>> {
//!         serde_json::from_slice(&self.buffer)
//!     }
//!
//!     // Convert to fully owned
//!     pub fn into_output(self) -> XrpcResult<R::Output<'static>> {
//!         let borrowed = self.parse()?;
//!         Ok(borrowed.into_static())
//!     }
//! }
//!
//! // Async method returns Response, caller chooses strategy
//! async fn send_xrpc<Req>(&self, req: Req) -> Result<Response<Req::Response>>
//! where
//!     Req: XrpcRequest<'_>
//! {
//!     // Do HTTP call, get Bytes buffer
//!     // Return Response wrapping that buffer
//!     // No lifetime issues - Response owns the buffer
//! }
//!
//! // Usage:
//! let response = send_xrpc(request).await?;
//!
//! // Zero-copy: borrow from response buffer
//! let output = response.parse()?;  // Output<'_> borrows from response
//!
//! // Or owned: convert to 'static
//! let output = response.into_output()?;  // Output<'static> is fully owned
//! ```
//!
//! When you see types like `Response<R: XrpcResp>` or methods with lifetime parameters,
//! this is the pattern at work. It looks a bit funky, but it's solving a specific problem
//! in a way that doesn't require unsafe code or much actual work from you, if you're using it.
//! It's also not too bad to write, once you're aware of the pattern and why it works. If you run
//! into a lifetime/borrowing inference issue in jacquard, please contact the crate author. She'd
//! be happy to debug, and if it's using a method from one of the jacquard crates and seems like
//! it *should* just work, that is a bug in jacquard, and you should [file an issue](https://tangled.org/@nonbinary.computer/jacquard/).

#![warn(missing_docs)]
pub use bytes;
pub use chrono;
pub use cowstr::CowStr;
pub use into_static::IntoStatic;
pub use smol_str;
pub use url;

/// A copy-on-write immutable string type that uses [`smol_str::SmolStr`] for
/// the "owned" variant.
#[macro_use]
pub mod cowstr;
#[macro_use]
/// Trait for taking ownership of most borrowed types in jacquard.
pub mod into_static;
pub mod error;
/// HTTP client abstraction used by jacquard crates.
pub mod http_client;
pub mod macros;
/// Generic session storage traits and utilities.
pub mod session;
/// Baseline fundamental AT Protocol data types.
pub mod types;
// XRPC protocol types and traits
pub mod xrpc;

/// Authorization token types for XRPC requests.
#[derive(Debug, Clone)]
pub enum AuthorizationToken<'s> {
    /// Bearer token (access JWT, refresh JWT to refresh the session)
    Bearer(CowStr<'s>),
    /// DPoP token (proof-of-possession) for OAuth
    Dpop(CowStr<'s>),
}

impl<'s> IntoStatic for AuthorizationToken<'s> {
    type Output = AuthorizationToken<'static>;
    fn into_static(self) -> AuthorizationToken<'static> {
        match self {
            AuthorizationToken::Bearer(token) => AuthorizationToken::Bearer(token.into_static()),
            AuthorizationToken::Dpop(token) => AuthorizationToken::Dpop(token.into_static()),
        }
    }
}
