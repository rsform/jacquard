# Changelog

## [0.5.0] - 2025-10-13

### Breaking Changes

**AgentSession trait** (`jacquard`)
- Removed `async fn` in favour of `impl Future` return types for better trait object compatibility
- Methods now return `impl Future` instead of being marked `async fn`

**XRPC improvements** (`jacquard-common`)
- Simplified response transmutation for typed record retrieval
- `Response::transmute()` added for zero-cost response type conversion

**jacquard-axum**
- Removed binary target (`main.rs`), now library-only

### Added

**Agent convenience methods** (`jacquard`)
- New `AgentSessionExt` trait automatically implemented for `AgentSession + IdentityResolver`
- **Basic CRUD**: `create_record()`, `get_record()`, `put_record()`, `delete_record()`
- **Update patterns**: `update_record()` (fetch-modify-put), `update_vec()`, `update_vec_item()`
- **Blob operations**: `upload_blob()`
- All methods auto-fill repo from session and collection from type's `Collection::NSID`
- Simplified bounds on `update_record` - no HRTB issues, works with all record types

**VecUpdate trait** (`jacquard`)
- `VecUpdate` trait for fetch-modify-put patterns on array-based endpoints
- `PreferencesUpdate` implementation for updating user preferences
- Enables type-safe updates to preferences, saved feeds, and other array endpoints

**Typed record retrieval** (`jacquard-api`, `jacquard-common`)
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
- `read_tangled_repo.rs`: Reading git repo metadata from tangled.sh
- `resolve_did.rs`: Identity resolution examples
- `public_atproto_feed.rs`: Unauthenticated feed access
- `axum_server.rs`: Server-side XRPC handler

### Changed

**Code organization** (`jacquard-lexicon`)
- Refactored monolithic `codegen.rs` into focused modules:
  - `codegen/structs.rs`: Record and object generation
  - `codegen/xrpc.rs`: XRPC request/response generation
  - `codegen/types.rs`: Type alias and union generation
  - `codegen/names.rs`: Identifier sanitization and naming
  - `codegen/lifetime.rs`: Lifetime propagation logic
  - `codegen/output.rs`: Module and feature generation
  - `codegen/utils.rs`: Shared utilities
- Improved code navigation and maintainability

**Documentation** (`jacquard`)
- Added comprehensive trait-level docs for `AgentSessionExt`
- Updated examples to use new convenience methods

### Fixed

- `update_record` now works with all record types without lifetime issues
- Proper `IdentityResolver` bounds on `AgentSessionExt`

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
