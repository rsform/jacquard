# Changelog

## [0.5.2] - 2025-10-14

### Added

**Value type deserialization** (`jacquard-common`)
- `from_json_value()`: Deserialize typed data directly from `serde_json::Value` without borrowing
- `from_data_owned()`, `from_raw_data_owned()`: Owned deserialization helpers
- `Data::from_json_owned()`: Parse JSON into owned `Data<'static>`
- `IntoStatic` implementation for `RawData` enabling owned conversions
- Re-exported value types from crate root for easier imports
- `Deserializer` trait implementations for `Data<'static>` and `RawData<'static>`
- Owned deserializer helpers: `OwnedArrayDeserializer`, `OwnedObjectDeserializer`, `OwnedBlobDeserializer`

**Service Auth** (`jacquard-axum`, `jacquard-common`)
- Full service authentication implementation for inter-service JWT verification
- `ExtractServiceAuth` Axum extractor for validating service auth tokens
- Axum service auth middleware
- JWT parsing and signature verification (ES256, ES256K)
- Service auth claims validation (issuer, audience, expiration, method binding)
- DID document resolution for signing key verification

**XrpcRequest derive macro** (`jacquard-derive`)
- `#[derive(XrpcRequest)]` for custom XRPC endpoints
- Automatically generates response marker struct and trait implementations
- Supports both client-side (`XrpcRequest`) and server-side (`XrpcEndpoint`) with `server` flag
- Simplifies defining custom XRPC endpoints outside of generated API

**Builder integration** (`jacquard-derive`)
- `#[lexicon]` macro now detects `bon::Builder` derive
- Automatically adds `#[builder(default)]` to `extra_data` field when Builder is present
- Makes `extra_data` optional in generated builders

### Fixed

**String deserialization** (`jacquard-common`)
- All string types (Did, Handle, Nsid, etc.) now properly handle URL-encoded values
- `serde_html_form` correctly decodes percent-encoded characters during deserialization
- Fixes issues with DIDs and other identifiers containing colons in query parameters

**Axum extractor** (`jacquard-axum`)
- Removed unnecessary URL-decoding workaround (now handled by improved string deserialization)
- Added comprehensive tests for URL-encoded query parameters
- Cleaner implementation with proper delegation to serde

### Changed

**Dependencies**
- Moved `clap` to dev-dependencies in `jacquard` (only used in examples)
- Moved `axum-macros` and `tracing-subscriber` to dev-dependencies in `jacquard-axum` (only used in examples)
- Removed unused dependencies: `urlencoding` (jacquard, jacquard-axum), `uuid` (jacquard-oauth), `serde_with` (jacquard-common)
- Removed `fancy` feature from `jacquard` (design smell for library crates)
- Moved various proc-macro crate dependencies to dev-dependencies in `jacquard-derive`

**Development tooling**
- Improved justfile with dynamic example discovery
- `just examples` now auto-discovers all examples
- `just example <name>` auto-detects package without manual configuration
- Better error messages when examples not found

**Documentation** (`jacquard`, `jacquard-common`)
- Improved lifetime pattern explanations
- Better documentation of zero-copy deserialization approach
- Links to docs.rs for generated documentation

---

## [0.5.1] - 2025-10-13

### Fixed

**Trait bounds** (`jacquard-common`)
- Removed lifetime parameter from `XrpcRequest` trait, simplifying trait bounds
- Lifetime now only appears on `XrpcEndpoint::Request<'de>` associated type
- Fixes issues with using XRPC types in async contexts like Axum extractors

### Changed

- Updated all workspace crates to 0.5.1 for consistency
- `jacquard-axum` remains at 0.5.1 (unchanged)

---

## `jacquard-axum` [0.5.1] - 2025-10-13

### Fixed

- Axum extractor now sets the correct Content-Type header during error path.

---

## [0.5.0] - 2025-10-13

### Added

**Agent convenience methods** (`jacquard`)
- New `AgentSessionExt` trait automatically implemented for `AgentSession + IdentityResolver`
- **Basic CRUD**: `create_record()`, `get_record()`, `put_record()`, `delete_record()`
- **Update patterns**: `update_record()` (fetch-modify-put), `update_vec()`, `update_vec_item()`
- **Blob operations**: `upload_blob()`
- All methods auto-fill repo from session or URI parameter as relevant, and collection from type's `Collection::NSID`

**VecUpdate trait** (`jacquard`)
- `VecUpdate` trait for fetch-modify-put patterns on array-based endpoints
- `PreferencesUpdate` implementation for updating Bluesky user preferences
- Enables simpler updates to preferences and other 'array of union' types

**Typed record retrieval** (`jacquard-api`, `jacquard-common`, `jacquard-lexicon`)
- Each collection generates `{Type}Record` marker struct implementing `XrpcResp`
- `Collection::Record` associated type points to the marker
- `get_record::<R>()` returns `Response<R::Record>` with zero-copy `.parse()`
- Response transmutation enables type-safe record operations

**Examples**
- `create_post.rs`: Creating posts with Agent convenience methods
- `update_profile.rs`: Updating profile with fetch-modify-put
- `post_with_image.rs`: Uploading images and creating posts with embeds
- `update_preferences.rs`: Using VecUpdate for preferences
- `create_whitewind_post.rs`, `read_whitewind_post.rs`: Third-party lexicons
- `read_tangled_repo.rs`: Reading git repo metadata from tangled.org
- `resolve_did.rs`: Identity resolution examples
- `public_atproto_feed.rs`: Unauthenticated feed access
- `axum_server.rs`: Server-side XRPC handler


**Documentation** (`jacquard`)
- A whole host of examples added, as well as a lengthy explainer of the trait patterns.

## [0.4.1] - 2025-10-13

### Added

**Collection trait improvements** (`jacquard-api`)
- Generated `{Type}Record` marker structs for all record types
- Each implements `XrpcResp` with `Output<'de> = {Type}<'de>` and `Err<'de> = RecordError<'de>`
- Enables typed `get_record` returning `Response<R::Record>`

### Changed

- Minor improvements to derive macros (`jacquard-derive`)
- Identity resolution refinements (`jacquard-identity`)
- OAuth client improvements (`jacquard-oauth`)

---

## [0.4.0] - 2025-10-11

### Breaking Changes

**Zero-copy deserialization** (`jacquard-common`, `jacquard-api`)
- `XrpcRequest` now takes a `'de` lifetime parameter and requires `Deserialize<'de>`
- For raw data, `Response::parse_data()` gives validated loosely-typed atproto data, while `Response::parse_raw()` gives the raw values, with minimal validation.

**XRPC module moved** (`jacquard-common`)
- `xrpc.rs` is now top-level instead of under `types`
- Import from `jacquard_common::xrpc::*`  not `jacquard_common::types::xrpc::*`

**Response API changes** (`jacquard-common`)
- `XrpcRequest::Output` and `XrpcRequest::Err` are associated types with lifetimes
- Split response and request traits: `XrpcRequest<'de>` for client, `XrpcEndpoint` for server
- Added `XrpcResp` marker trait

**Various traits** (`jacquard`, `jacquard-common`, `jacquard-lexicon`, `jacquard-oauth`)
- Removed #[async_trait] attribute macro usage in favour of `impl Future` return types with manual bounds.
- Boxing imposed by asyc_trait negatively affected borrowing modes in async methods.
- Currently no semver guarantees on API trait bounds, if they need to tighten, they will.

### Added

**New crate: `jacquard-axum`**
- Server-side XRPC handlers for Axum
- `ExtractXrpc<R>` deserializes incoming requests (query params for Query, body for Procedure)
- Automatic error responses

**Lexicon codegen fixes** (`jacquard-lexicon`)
- Union variant collision detection: when multiple namespaces have similar type names, foreign ones get prefixed (e.g., `Images` vs `BskyImages`)
- Token types generate unit structs with `Display` instead of being skipped
- Namespace dependency tracking during union generation
- `generate_cargo_features()` outputs Cargo.toml features with correct deps
- `sanitize_name()` ensures valid Rust identifiers

**Lexicons** (`jacquard-api`)

Added 646 lexicon schemas. Highlights:

Core ATProto:
- `com.atproto.*`
- `com.bad-example.*` for identity resolution

Bluesky:
- `app.bsky.*` bluesky app
- `chat.bsky.*` chat client
- `tools.ozone.*` moderation

Third-party:
- `sh.tangled.*` - git forge
- `sh.weaver.*` - orual's WIP markdown blog platform
- `pub.leaflet.*` - longform publishing
- `net.anisota.*` - gamified and calming take on bluesky
- `network.slices.*` - serverless atproto hosting
- `tools.smokesignal.*` - automation
- `com.whtwnd.*` - markdown blogging
- `place.stream.*` - livestreaming
- `blue.2048.*` - 2048 game
- `community.lexicon.*` - community extensions (bookmarks, calendar, location, payments)
- `my.skylights.*` - media tracking
- `social.psky.*` - social extensions
- `blue.linkat.*` - link boards

Plus 30+ more experimental/community namespaces.

**Value types** (`jacquard-common`)
- `RawData` to `Data` conversion with type inference
- `from_data`, `from_raw_data`, `to_data`, and `to_raw_data` to serialize to and deserialize from the loosely typed value data formats. Particularly useful for second-stage deserialization of type "unknown" fields in lexicons, such as `PostView.record`.

### Changed

- `generate_union()` takes current NSID for dependency tracking
- Generated code uses `sanitize_name()` for identifiers more consistently
- Added derive macro for IntoStatic trait implementation

### Fixed

- Methods to extract the output from an XRPC response now behave well with respect to lifetimes and borrowing.
- Now possible to use jacquard types in places like axum extractors due to lifetime improvements
- Union variants don't collide when multiple namespaces define similar types and another namespace includes them

---
