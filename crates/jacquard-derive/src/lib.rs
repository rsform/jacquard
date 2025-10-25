//! # Derive macros for jacquard lexicon types
//!
//! This crate provides attribute and derive macros for working with Jacquard types.
//! The code generator uses `#[lexicon]` and `#[open_union]` to add lexicon-specific behavior.
//! You'll use `#[derive(IntoStatic)]` frequently, and `#[derive(XrpcRequest)]` when defining
//! custom XRPC endpoints.
//!
//! ## Macros
//!
//! ### `#[lexicon]`
//!
//! Adds an `extra_data` field to structs to capture unknown fields during deserialization.
//! This makes objects "open" - they'll accept and preserve fields not defined in the schema.
//!
//! ```ignore
//! #[lexicon]
//! struct Post<'s> {
//!     text: &'s str,
//! }
//! // Expands to add:
//! // #[serde(flatten)]
//! // pub extra_data: BTreeMap<SmolStr, Data<'s>>
//! ```
//!
//! ### `#[open_union]`
//!
//! Adds an `Unknown(Data)` variant to enums to make them extensible unions. This lets
//! enums accept variants not defined in your code, storing them as loosely typed atproto `Data`.
//!
//! ```ignore
//! #[open_union]
//! enum RecordEmbed<'s> {
//!     #[serde(rename = "app.bsky.embed.images")]
//!     Images(Images),
//! }
//! // Expands to add:
//! // #[serde(untagged)]
//! // Unknown(Data<'s>)
//! ```
//!
//! ### `#[derive(IntoStatic)]`
//!
//! Derives conversion from borrowed (`'a`) to owned (`'static`) types by recursively calling
//! `.into_static()` on all fields. Works with structs and enums.
//!
//! ```ignore
//! #[derive(IntoStatic)]
//! struct Post<'a> {
//!     text: CowStr<'a>,
//! }
//! // Generates:
//! // impl IntoStatic for Post<'_> {
//! //     type Output = Post<'static>;
//! //     fn into_static(self) -> Self::Output { ... }
//! // }
//! ```
//!
//! ### `#[derive(XrpcRequest)]`
//!
//! Derives XRPC request traits for custom endpoints. Generates the response marker struct
//! and implements `XrpcRequest` (and optionally `XrpcEndpoint` for server-side).
//!
//! ```ignore
//! #[derive(Serialize, Deserialize, XrpcRequest)]
//! #[xrpc(
//!     nsid = "com.example.getThing",
//!     method = Query,
//!     output = GetThingOutput,
//! )]
//! struct GetThing<'a> {
//!     #[serde(borrow)]
//!     pub id: CowStr<'a>,
//! }
//! // Generates:
//! // - GetThingResponse struct
//! // - impl XrpcResp for GetThingResponse
//! // - impl XrpcRequest for GetThing
//! ```

use proc_macro::TokenStream;

/// Attribute macro that adds an `extra_data` field to structs to capture unknown fields
/// during deserialization.
///
/// See crate documentation for examples.
#[proc_macro_attribute]
pub fn lexicon(attr: TokenStream, item: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_lexicon(attr.into(), item.into()).into()
}

/// Attribute macro that adds an `Unknown(Data)` variant to enums to make them open unions.
///
/// See crate documentation for examples.
#[proc_macro_attribute]
pub fn open_union(attr: TokenStream, item: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_open_union(attr.into(), item.into()).into()
}

/// Derive macro for `IntoStatic` trait.
///
/// Automatically implements conversion from borrowed to owned ('static) types.
/// See crate documentation for examples.
#[proc_macro_derive(IntoStatic)]
pub fn derive_into_static(input: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_derive_into_static(input.into()).into()
}

/// Derive macro for `XrpcRequest` trait.
///
/// Automatically generates the response marker struct, `XrpcResp` impl, and `XrpcRequest` impl
/// for an XRPC endpoint. See crate documentation for examples.
#[proc_macro_derive(XrpcRequest, attributes(xrpc))]
pub fn derive_xrpc_request(input: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_derive_xrpc_request(input.into()).into()
}
