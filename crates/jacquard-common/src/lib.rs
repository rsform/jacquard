//! Common types for the jacquard implementation of atproto
//!
//! ## Just `.send()` it
//!
//! Jacquard has a couple of `.send()` methods. One is stateless. it's the output of a method that creates a request builder, implemented as an extension trait, `XrpcExt`, on any http client which implements a very simple HttpClient trait. You can use a bare `reqwest::Client` to make XRPC requests. You call `.xrpc(base_url)` and get an `XrpcCall` struct. `XrpcCall` is a builder, which allows you to pass authentication, atproto proxy settings, labeler headings, and set other options for the final request. There's also a similar trait `DpopExt` in the `jacquard-oauth` crate, which handles that form of authenticated request in a similar way. For basic stuff, this works great, and it's a useful building block for more complex logic, or when one size does **not** in fact fit all.
//!
//! ```ignore
//! use jacquard_common::xrpc::XrpcExt;
//! use jacquard_common::http_client::HttpClient;
//! // ...
//! let http = reqwest::Client::new();
//! let base = url::Url::parse("https://public.api.bsky.app")?;
//! let resp = http.xrpc(base).send(&request).await?;
//! ```
//! The other, `XrpcClient`, is stateful, and can be implemented on anything with a bit of internal state to store the base URI (the URL of the PDS being contacted) and the default options. It's the one you're most likely to interact with doing normal atproto API client stuff. The Agent struct in the initial example implements that trait, as does the session struct it wraps, and the `.send()` method used is that trait method.
//!
//! >`XrpcClient` implementers don't *have* to implement token auto-refresh and so on, but realistically they *should* implement at least a basic version. There is an `AgentSession` trait which does require full session/state management.
//!
//! Here is the entire text of `XrpcCall::send()`. [`build_http_request()`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-common/src/xrpc.rs#L400) and [`process_response()`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-common/src/xrpc.rs#L344) are public functions and can be used in other crates. The first does more or less what it says on the tin. The second does less than you might think. It mostly surfaces authentication errors at an earlier level so you don't have to fully parse the response to know if there was an error or not.
//!
//! ```ignore
//! pub async fn send<R>(
//!         self,
//!         request: &R,
//!     ) -> XrpcResult<Response<<R as XrpcRequest>::Response>>
//!     where
//!         R: XrpcRequest,
//!     {
//!         let http_request = build_http_request(&self.base, request, &self.opts)
//!             .map_err(TransportError::from)?;
//!         let http_response = self
//!             .client
//!             .send_http(http_request)
//!             .await
//!             .map_err(|e| TransportError::Other(Box::new(e)))?;
//!         process_response(http_response)
//!     }
//! ```
//! >A core goal of Jacquard is to not only provide an easy interface to atproto, but to also make it very easy to build something that fits your needs, and making "helper" functions like those part of the API surface is a big part of that, as are "stateless" implementations like `XrpcExt` and `XrpcCall`.
//!
//! `.send()` works for any endpoint and any type that implements the required traits, regardless of what crate it's defined in. There's no `KnownRecords` enum which defines a complete set of known records, and no restriction of Service endpoints in the agent/client, or anything like that, nothing that privileges any set of lexicons or way of working with the library, as much as possible. There's one primary method and you can put pretty much anything relevant into it. Whatever atproto API you need to call, just `.send()` it. Okay there are a couple of additional helpers, but we're focusing on the core one, because pretty much everything else is just wrapping the above `send()` in one way or another, and they use the same pattern.
//!
//! ## Punchcard Instructions
//!
//! So how does this work? How does `send()` and its helper functions know what to do? The answer shouldn't be surprising to anyone familiar with Rust. It's traits! Specifically, the following traits, which have generated implementations for every lexicon type ingested by Jacquard's API code generation, but which honestly aren't hard to just implement yourself (more tedious than anything). XrpcResp is always implemented on a unit/marker struct with no fields. They provide all the request-specific instructions to the functions.
//!
//! ```ignore
//! pub trait XrpcRequest: Serialize {
//!     const NSID: &'static str;
//!     /// XRPC method (query/GET or procedure/POST)
//!     const METHOD: XrpcMethod;
//!     type Response: XrpcResp;
//!     /// Encode the request body for procedures.
//!     fn encode_body(&self) -> Result<Vec<u8>, EncodeError> {
//!         Ok(serde_json::to_vec(self)?)
//!     }
//!     /// Decode the request body for procedures. (Used server-side)
//!     fn decode_body<'de>(body: &'de [u8]) -> Result<Box<Self>, DecodeError>
//!     where
//!         Self: Deserialize<'de>
//!     {
//!         let body: Self = serde_json::from_slice(body).map_err(|e| DecodeError::Json(e))?;
//!         Ok(Box::new(body))
//!     }
//! }
//! pub trait XrpcResp {
//!     const NSID: &'static str;
//!     /// Output encoding (MIME type)
//!     const ENCODING: &'static str;
//!     type Output<'de>: Deserialize<'de> + IntoStatic;
//!     type Err<'de>: Error + Deserialize<'de> + IntoStatic;
//! }
//! ```
//! Here are the implementations for [`GetTimeline`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-api/src/app_bsky/feed/get_timeline.rs). You'll also note that `send()` doesn't return the fully decoded response on success. It returns a Response struct which has a generic parameter that must implement the XrpcResp trait above. Here's its definition. It's essentially just a cheaply cloneable byte buffer and a type marker.
//!
//! ```ignore
//! pub struct Response<R: XrpcResp> {
//!     buffer: Bytes,
//!     status: StatusCode,
//!     _marker: PhantomData<R>,
//! }
//!
//! impl<R: XrpcResp> Response<R> {
//!     pub fn parse<'s>(
//!         &'s self
//!     ) -> Result<<Resp as XrpcResp>::Output<'s>, XrpcError<<Resp as XrpcResp>::Err<'s>>> {
//!         // Borrowed parsing into Output or Err
//!     }
//!     pub fn into_output(
//!         self
//!     ) -> Result<<Resp as XrpcResp>::Output<'static>, XrpcError<<Resp as XrpcResp>::Err<'static>>>
//!     where ...
//!     {  /* Owned parsing into Output or Err */  }
//! }
//! ```
//! You decode the response (or the endpoint-specific error) out of this, borrowing from the buffer or taking ownership so you can drop the buffer. There are two reasons for this. One is separation of concerns. By two-staging the parsing, it's easier to distinguish network and authentication problems from application-level errors. The second is lifetimes and borrowed deserialization.
//!
//! ## Working with Lifetimes and Zero-Copy Deserialization
//!
//! Jacquard is designed around zero-copy/borrowed deserialization: types like [`Post<'a>`](https://tangled.org/@nonbinary.computer/jacquard/blob/main/crates/jacquard-api/src/app_bsky/feed/post.rs) can borrow strings and other data directly from the response buffer instead of allocating owned copies. This is great for performance, but it creates some interesting challenges, especially in async contexts. So how do you specify the lifetime of the borrow?
//!
//! The naive approach would be to put a lifetime parameter on the trait itself:
//!
//!```ignore
//!// This looks reasonable but creates problems in generic/async contexts
//!trait NaiveXrpcRequest<'de> {
//!    type Output: Deserialize<'de>;
//!    // ...
//!}
//!```
//!
//! This looks reasonable until you try to use it in a generic context. If you have a function that works with *any* lifetime, you need a Higher-ranked trait bound:
//!
//!```ignore
//!fn parse<R>(response: &[u8]) ... // return type
//!where
//!    R: for<'any> XrpcRequest<'any>
//!{ /*  deserialize from response... */  }
//!```
//!
//! The `for<'any>` bound says "this type must implement `XrpcRequest` for *every possible lifetime*", which, for `Deserialize`, is effectively the same as requiring `DeserializeOwned`. You've probably just thrown away your zero-copy optimization, and furthermore that trait bound just straight-up won't work on most of the types in Jacquard. The vast majority of them have either a custom Deserialize implementation which will borrow if it can, a `#[serde(borrow)]` attribute on one or more fields, or an equivalent lifetime bound attribute, associated with the Deserialize derive macro. You will get "Deserialize implementation not general enough" if you try. And no, you cannot have an additional deserialize implementation for the `'static` lifetime due to how serde works.
//!
//! If you instead try something like the below function signature and specify a specific lifetime, it will compile in isolation, but when you go to use it, the Rust compiler will not generally be able to figure out the lifetimes at the call site, and will complain about things being dropped while still borrowed, even if you convert the response to an owned/ `'static` lifetime version of the type.
//!
//!```ignore
//!fn parse<'s, R: XrpcRequest<'s>>(response: &'s [u8]) ... // return type with the same lifetime
//!{ /*  deserialize from response... */  }
//!```
//!
//! It gets worse with async. If you want to return borrowed data from an async method, where does the lifetime come from? The response buffer needs to outlive the borrow, but the buffer is consumed or potentially has to have an unbounded lifetime. You end up with confusing and frustrating errors because the compiler can't prove the buffer will stay alive or that you have taken ownership of the parts of it you care about. You *could* do some lifetime laundering with `unsafe`, but that road leads to potential soundness issues, and besides, you don't actually *need* to tell `rustc` to "trust me, bro", you can, with some cleverness, explain this to the compiler in a way that it can reason about perfectly well.
//!
//! ### Explaining where the buffer goes to `rustc`
//!
//! The fix is to use Generic Associated Types (GATs) on the trait's associated types, while keeping the trait itself lifetime-free:
//!
//!```ignore
//!pub trait XrpcResp {
//!    const NSID: &'static str;
//!    /// Output encoding (MIME type)
//!    const ENCODING: &'static str;
//!    type Output<'de>: Deserialize<'de> + IntoStatic;
//!    type Err<'de>: Error + Deserialize<'de> + IntoStatic;
//!}
//!```
//!
//!Now you can write trait bounds without HRTBs, and with lifetime bounds that are actually possible for Jacquard's borrowed deserializing types to meet:
//!
//!```ignore
//!fn parse<'s, R: XrpcResp>(response: &'s [u8]) /* return type with same lifetime */ {
//!    // Compiler can pick a concrete lifetime for R::Output<'_> or have it specified easily
//!}
//!```
//!
//!Methods that need lifetimes use method-level generic parameters:
//!
//!```ignore
//!// This is part of a trait from jacquard itself, used to genericize updates to things like the Bluesky
//!// preferences union, so that if you implement a similar lexicon type in your app, you don't have
//!// to special-case it. Instead you can do a relatively simple trait implementation and then call
//!// .update_vec() with a modifier function or .update_vec_item() with a single item you want to set.
//
//!pub trait VecUpdate {
//!    type GetRequest: XrpcRequest;
//!    type PutRequest: XrpcRequest;
//!    //... more stuff
//
//!    //Method-level lifetime, GAT on response type
//!    fn extract_vec<'s>(
//!        output: <<Self::GetRequest as XrpcRequest>::Response as XrpcResp>::Output<'s>
//!    ) -> Vec<Self::Item>;
//!    //... more stuff
//!}
//!```
//!
//!The compiler can monomorphize for concrete lifetimes instead of trying to prove bounds hold for *all* lifetimes at once, or struggle to figure out when you're done with a buffer. `XrpcResp` being separate and lifetime-free lets async methods like `.send()` return a `Response` that owns the response buffer, and then the *caller* decides the lifetime strategy:
//!
//!```ignore
//!// Zero-copy: borrow from the owned buffer
//!let output: R::Output<'_> = response.parse()?;
//
//!// Owned: convert to 'static via IntoStatic
//!let output: R::Output<'static> = response.into_output()?;
//!```
//!
//! The async method doesn't need to know or care about lifetimes for the most part - it just returns the `Response`. The caller gets full control over whether to use borrowed or owned data. It can even decide after the fact that it doesn't want to parse out the API response type that it asked for. Instead it can call `.parse_data()` or `.parse_raw()` on the response to get loosely typed, validated data or minimally typed maximally accepting data values out.
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
/// Service authentication JWT parsing and verification.
#[cfg(feature = "service-auth")]
pub mod service_auth;
/// Generic session storage traits and utilities.
pub mod session;
/// Baseline fundamental AT Protocol data types.
pub mod types;
// XRPC protocol types and traits
pub mod xrpc;
/// Stream abstractions for HTTP request/response bodies.
#[cfg(feature = "streaming")]
pub mod stream;

#[cfg(feature = "streaming")]
pub use stream::{ByteStream, ByteSink, StreamError, StreamErrorKind};

#[cfg(feature = "streaming")]
pub use xrpc::StreamingResponse;

pub use types::value::*;

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

/// Serde helper for deserializing stuff when you want an owned version
pub fn deserialize_owned<'de, T, D>(deserializer: D) -> Result<<T as IntoStatic>::Output, D::Error>
where
    T: serde::Deserialize<'de> + IntoStatic,
    D: serde::Deserializer<'de>,
{
    let value = T::deserialize(deserializer)?;
    Ok(value.into_static())
}
