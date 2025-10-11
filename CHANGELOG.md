# Changelog

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
