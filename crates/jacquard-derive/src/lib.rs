//! # Derive macros for jacquard lexicon types
//!
//! This crate provides attribute and derive macros for working with Jacquard types.
//! The code generator uses `#[lexicon]` and `#[open_union]` to add lexicon-specific behavior.
//! You'll use `#[derive(IntoStatic)]` frequently, `#[derive(XrpcRequest)]` when defining
//! custom XRPC endpoints, and `#[derive(LexiconSchema)]` for reverse codegen (Rust → lexicon).
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
//!
//! ### `#[derive(LexiconSchema)]`
//!
//! Derives `LexiconSchema` trait for reverse codegen (Rust → lexicon JSON). Generate
//! lexicon schemas from your Rust types for rapid prototyping and custom lexicons.
//!
//! **Type-level attributes** (`#[lexicon(...)]`):
//! - `nsid = "..."`: The lexicon NSID (required)
//! - `record`: Mark as a record type
//! - `key = "..."`: Record key type (`"tid"`, `"literal:self"`, or custom) - optional
//! - `object`: Mark as an object type (default if neither record/procedure/query)
//! - `fragment = "..."`: Fragment name for non-main defs (e.g., `fragment = "textSlice"`)
//!
//! **Field-level attributes** (`#[lexicon(...)]`):
//! - `max_length = N`: Max byte length for strings
//! - `max_graphemes = N`: Max grapheme count for strings
//! - `min_length = N`, `min_graphemes = N`: Minimum constraints
//! - `minimum = N`, `maximum = N`: Integer range constraints
//! - `max_items = N`: Max array length
//! - `item_max_length = N`, `item_max_graphemes = N`: Constraints on array items
//! - `ref = "..."`: Explicit type ref (e.g., `ref = "com.atproto.repo.strongRef"` or `ref = "#textSlice"`)
//! - `union`: Mark field as union type (use with `#[lexicon_union]` enum)
//!
//! **Serde integration**: Respects `#[serde(rename)]`, `#[serde(rename_all)]`, and
//! `#[serde(skip)]`. Defaults to camelCase for field names (lexicon standard).
//!
//! **Unions**: Use `#[lexicon_union]` attribute macro, not `#[derive(LexiconSchema)]`.
//! Mark union fields with `#[lexicon(union)]`.
//!
//! ```ignore
//! // Record with constraints and fragments
//! #[derive(LexiconSchema)]
//! #[lexicon(nsid = "app.bsky.feed.post", record, key = "tid")]
//! #[serde(rename_all = "camelCase")]
//! struct Post<'a> {
//!     #[lexicon(max_graphemes = 300, max_length = 3000)]
//!     pub text: CowStr<'a>,
//!
//!     pub created_at: Datetime,  // -> "createdAt" (camelCase)
//!
//!     #[lexicon(union)]
//!     pub embed: Option<PostEmbed<'a>>,
//!
//!     #[lexicon(max_items = 8, item_max_length = 640, item_max_graphemes = 64)]
//!     pub tags: Option<Vec<CowStr<'a>>>,
//!
//!     #[lexicon(ref = "app.bsky.richtext.facet")]
//!     pub facets: Option<Vec<CowStr<'a>>>,
//! }
//!
//! // Fragment (non-main def)
//! #[derive(LexiconSchema)]
//! #[lexicon(nsid = "app.bsky.feed.post", fragment = "textSlice")]
//! #[serde(rename_all = "camelCase")]
//! struct TextSlice {
//!     #[lexicon(minimum = 0)]
//!     pub start: i64,
//!     #[lexicon(minimum = 0)]
//!     pub end: i64,
//! }
//!
//! // Union (uses lexicon_union, not LexiconSchema)
//! #[lexicon_union]
//! #[serde(tag = "$type")]
//! enum PostEmbed<'a> {
//!     #[serde(rename = "app.bsky.embed.images")]
//!     Images(CowStr<'a>),
//!     #[serde(rename = "app.bsky.embed.video")]
//!     Video(CowStr<'a>),
//! }
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

/// Derive macro for `LexiconSchema` trait.
///
/// Generates `LexiconSchema` trait impl from Rust types for reverse codegen (Rust → lexicon JSON).
/// Produces lexicon schema definitions and runtime validation code from your type definitions.
///
/// **What it generates:**
/// - `impl LexiconSchema` with `nsid()`, `schema_id()`, and `lexicon_doc()` methods
/// - `validate()` method that checks constraints at runtime
/// - `inventory::submit!` registration for schema discovery
///
/// **Attributes:** `#[lexicon(...)]` and `#[nsid = "..."]` on types and fields.
/// See crate docs for full attribute reference and examples.
#[proc_macro_derive(LexiconSchema, attributes(lexicon, nsid))]
pub fn derive_lexicon_schema(input: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_derive_lexicon_schema(input.into()).into()
}

/// Attribute macro for union enums.
///
/// Marks an enum as a lexicon union type and generates a const containing the union refs
/// extracted from `#[nsid = "..."]` or `#[serde(rename = "...")]` attributes on variants.
///
/// ```ignore
/// #[lexicon_union]
/// #[serde(tag = "$type")]
/// pub enum PostEmbed<'a> {
///     #[serde(rename = "app.bsky.embed.images")]
///     Images(Images<'a>),
///     #[nsid = "app.bsky.embed.video"]
///     Video(Video<'a>),
/// }
/// // Generates: PostEmbed::LEXICON_UNION_REFS const
/// ```
#[proc_macro_attribute]
pub fn lexicon_union(attr: TokenStream, item: TokenStream) -> TokenStream {
    jacquard_lexicon::derive_impl::impl_lexicon_union(attr.into(), item.into()).into()
}
