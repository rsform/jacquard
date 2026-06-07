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
//! let base = jacquard_common::deps::fluent_uri::Uri::parse("https://public.api.bsky.app")?.to_owned();
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
//!     type Output<S: BosStr>;
//!     type Err: Error + Serialize + DeserializeOwned;
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
//!     pub fn parse<'s, S>(&'s self) -> Result<R::Output<S>, XrpcError<R::Err>>
//!     where
//!         S: BosStr + Deserialize<'s>,
//!         R::Output<S>: Deserialize<'s>,
//!     {
//!         // Parse with the caller's chosen string backing.
//!     }
//!     pub fn into_output(self) -> Result<R::Output<SmolStr>, XrpcError<R::Err>>
//!     where
//!         R::Output<SmolStr>: DeserializeOwned,
//!     {
//!         // Parse into owned/default-backed output.
//!     }
//! }
//! ```
//! You decode the response (or the endpoint-specific error) out of this when you are ready. There
//! are two reasons for this. One is separation of concerns: by two-staging the parsing, it is easier
//! to distinguish network and authentication problems from application-level errors. The second is
//! string backing: callers can choose ordinary owned output or explicit borrowed parsing.
//!
//! ## String backing, borrowing, and response parsing
//!
//! Most generated output types are parameterized over a string backing type: `Output<S: BosStr>`.
//! The usual path is owned output:
//!
//! ```ignore
//! let output = response.into_output()?;
//! ```
//!
//! `into_output()` parses into `SmolStr`-backed output. This is the convenient default when values
//! need to be stored, moved independently of the response buffer, or passed through frameworks and
//! APIs that require `DeserializeOwned`.
//!
//! When you specifically want to borrow from the response buffer, choose a borrowed or
//! borrow-or-own backing at parse time:
//!
//! ```ignore
//! let output = response.parse::<CowStr<'_>>()?;
//! let output = response.parse::<&str>()?;
//! ```
//!
//! Borrowed output works fine across async code as long as the value remains tied to the
//! buffer-owning `Response`. The `.send()` method itself can stay lifetime-free because it returns
//! that buffer-owning response, and the caller decides whether to parse into owned data or borrow
//! from the buffer.
//!
//! `XrpcResp` stays lifetime-free too. Its success output is a GAT over the backing string type,
//! and its error type is a plain owned type:
//!
//! ```ignore
//! pub trait XrpcResp {
//!     const NSID: &'static str;
//!     const ENCODING: &'static str;
//!     type Output<S: BosStr>;
//!     type Err: Error + Serialize + DeserializeOwned;
//! }
//! ```
//!
//! Keeping endpoint errors owned avoids lifetime gymnastics on the unhappy path. If you do not want
//! to parse the endpoint-specific success type, `Response` also supports `.parse_data()` and
//! `.parse_raw()` for loosely typed atproto data.

#![no_std]
#![warn(missing_docs)]

#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

/// Lazy initialization type for static values.
///
/// Uses `std::sync::LazyLock` when the `std` feature is enabled, or `spin::Lazy`
/// for no_std environments. Exported so downstream crates can use it without
/// their own conditional compilation.
#[cfg(feature = "std")]
pub type Lazy<T> = std::sync::LazyLock<T>;

#[cfg(not(feature = "std"))]
pub use spin::Lazy;

pub use bos::{BorrowOrShare, Bos, BosStr, DefaultStr, FromStaticStr};
pub use cowstr::CowStr;
pub use into_static::IntoStatic;

/// A copy-on-write immutable string type that uses [`smol_str::SmolStr`] for
/// the "owned" variant.
#[macro_use]
pub mod cowstr;
#[macro_use]
/// Trait for taking ownership of most borrowed types in jacquard.
pub mod into_static;
/// Borrow-or-share traits for abstracting over owned and borrowed string representations.
#[macro_use]
pub mod bos;
/// Re-exports of external crate dependencies for consistent access across jacquard.
pub mod deps;
pub mod error;
pub mod http_client;
pub mod macros;
pub mod opt_serde_bytes_helper;
pub mod serde_bytes_helper;
#[cfg(feature = "service-auth")]
pub mod service_auth;
pub mod session;
#[cfg(feature = "streaming")]
pub mod stream;
/// Compile-time TLD lookup for disambiguating handles from NSIDs.
pub(crate) mod tld;
/// Baseline fundamental AT Protocol data types.
pub mod types;
pub mod xrpc;

#[cfg(feature = "streaming")]
pub use stream::{ByteSink, ByteStream, StreamError, StreamErrorKind};

#[cfg(feature = "streaming")]
pub use xrpc::StreamingResponse;

#[cfg(feature = "websocket")]
pub mod websocket;

#[cfg(feature = "websocket")]
pub mod jetstream;

#[cfg(feature = "websocket")]
pub use websocket::{
    CloseCode, CloseFrame, WebSocketClient, WebSocketConnection, WsMessage, WsSink, WsStream,
    WsText, tungstenite_client::TungsteniteClient,
};

pub use types::value::*;

/// Authorization token types for XRPC requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorizationToken<S: BosStr = DefaultStr> {
    /// Bearer token (access JWT, refresh JWT to refresh the session)
    Bearer(S),
    /// DPoP token (proof-of-possession) for OAuth
    Dpop(S),
}

impl<S: BosStr> AuthorizationToken<S> {
    /// Borrows the token as a `&str` reference.
    pub fn borrow(&self) -> AuthorizationToken<&str> {
        match self {
            AuthorizationToken::Bearer(token) => {
                AuthorizationToken::Bearer(token.borrow_or_share())
            }
            AuthorizationToken::Dpop(token) => AuthorizationToken::Dpop(token.borrow_or_share()),
        }
    }
}

impl<S: BosStr> IntoStatic for AuthorizationToken<S>
where
    S: IntoStatic,
    S::Output: BosStr,
{
    type Output = AuthorizationToken<S::Output>;
    fn into_static(self) -> AuthorizationToken<S::Output> {
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
