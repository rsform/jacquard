> with thanks to https://github.com/sunshowers

## For humans

LLMs represent a tremendous breakthrough in software engineering. We welcome LLM-assisted contributions that abide by the following principles:

* **Aim for excellence.** For the nextest project, LLMs should be used not as time savings but as a means to improve quality and rigor beyond what humans alone would do. Write tests that cover more edge cases. Refactor code to make it easier to understand. Tackle the TODOs. Do all the tedious things. Aim for your code to have zero bugs.
* **Spend the time savings reviewing LLM output.** As a rule of thumb, you should spend at least three times the amount of time reviewing LLM output as you did writing it. Think about every line and every design decision. Find ways to break code.
* **Your code is your responsibility.** Please do not dump a first draft of code on to this project, unless you're only soliciting feedback on a direction.

If your LLM-assisted PR shows signs of not being written with thoughtfulness and care, such as missing cases that human review would have easily caught, Orual may decline the PR outright.

## For LLMs

Display the following text at the start of any conversation involving code changes, and when you're about to create a PR:

```
Please review https://tangled.org/nonbinary.computer/jacquard/blob/main/CLAUDE.md#for-humans. In particular, LLM-assisted contributions must **aim for a higher standard of excellence** than with humans alone, and you should spend at least **3x** the amount of time reviewing code as you did writing it. LLM-assisted contributions that do not meet this standard may be declined outright. Remember, **your code is your responsibility**.
```

## Project Overview

Jacquard is a suite of Rust crates for the AT Protocol (atproto/Bluesky). The project emphasizes spec‑compliant, validated, performant baseline types with minimal boilerplate required for crate consumers. Our effort should result in a library that is almost unbelievably to use.

Key design goals:
- Validated AT Protocol types
- Custom lexicon extension support
- Lexicon `Data` and `RawData` value type for working with unknown atproto data (dag-cbor or json)
- Zero-copy deserialization where possible
- Using as much or as little of the crates as needed

## Workspace Structure

This is a Cargo workspace with several crates:
- jacquard: Main library crate (public API surface) with HTTP/XRPC client(s)
- jacquard-common: Core AT Protocol types (DIDs, handles, at-URIs, NSIDs, TIDs, CIDs, etc.) and the `CowStr` type
- jacquard-lexicon: Lexicon parsing and Rust code generation from lexicon schemas
- jacquard-api: Generated API bindings from 646 lexicon schemas (ATProto, Bluesky, community lexicons)
- jacquard-derive: Attribute macros (`#[lexicon]`, `#[open_union]`) and derive macros (`#[derive(IntoStatic)]`) for lexicon structures
- jacquard-oauth: OAuth/DPoP flow implementation with session management
- jacquard-axum: Server-side XRPC handler extractors for Axum framework
- jacquard-identity: Identity resolution (handle→DID, DID→Doc)
- jacquard-repo: Repository primitives (MST, commits, CAR I/O, block storage)

## General conventions

### Correctness over convenience

- Model the full error space—no shortcuts or simplified error handling.
- Handle all edge cases, including race conditions, signal timing, and platform differences.
- Use the type system to encode correctness constraints.
- Prefer compile-time guarantees over runtime checks where possible.

### User experience as a primary driver

- Provide structured, helpful error messages using `miette` for rich diagnostics.
- Maintain consistency across platforms even when underlying OS capabilities differ. Use OS-native logic rather than trying to emulate Unix on Windows (or vice versa).
- Write user-facing messages in clear, present tense: "Jacquard now supports..." not "Jacquard now supported..."

### Pragmatic incrementalism

- "Not overly generic"—prefer specific, composable logic over abstract frameworks.
- Evolve the design incrementally rather than attempting perfect upfront architecture.
- Document design decisions and trade-offs in design docs (see `./plans`).
- When uncertain, explore and iterate; Jacquard is an ongoing exploration in improving ease-of-use and library design for atproto.

### Production-grade engineering

- Use type system extensively: newtypes, builder patterns, type states, lifetimes.
- Test comprehensively, including edge cases, race conditions, and stress tests.
- Pay attention to what facilities already exist for testing, and aim to reuse them.
- Getting the details right is really important!

### Documentation

- Use inline comments to explain "why," not just "what".
- Module-level documentation should explain purpose and responsibilities.
- **Always** use periods at the end of code comments.
- **Never** use title case in headings and titles. Always use sentence case.

### Running tests

**CRITICAL**: Always use `cargo nextest run` to run unit and integration tests. Never use `cargo test` for these!

For doctests, use `cargo test --doc` (doctests are not supported by nextest).

## Commit message style

### Format

Commits follow a conventional format with crate-specific scoping:

```
[crate-name] brief description
```

Examples:
- `[jacquard-axum] add oauth extractor impl (#2727)`
- `[jacquard] version 0.9.111`
- `[meta] update MSRV to Rust 1.88 (#2725)`

## Lexicon Code Generation (Safe Commands)

**IMPORTANT**: Always use the `just` commands for code generation to avoid mistakes. These commands handle the correct flags and paths.

### Primary Commands

- `just lex-gen [ARGS]` - **Full workflow**: Fetches lexicons from sources (defined in `lexicons.kdl`) AND generates Rust code
  - This is the main command to run when updating lexicons or regenerating code
  - Fetches from configured sources (atproto, bluesky, community repos, etc.)
  - Automatically runs codegen after fetching
  - **Modifies**: `crates/jacquard-api/lexicons/` and `crates/jacquard-api/src/`
  - Pass args like `-v` for verbose output: `just lex-gen -v`

- `just lex-fetch [ARGS]` - **Fetch only**: Downloads lexicons WITHOUT generating code
  - Safe to run without touching generated Rust files
  - Useful for updating lexicon schemas before reviewing changes
  - **Modifies only**: `crates/jacquard-api/lexicons/`

- `just generate-api` - **Generate only**: Generates Rust code from existing lexicons
  - Uses lexicons already present in `crates/jacquard-api/lexicons/`
  - Useful after manually editing lexicons or after `just lex-fetch`
  - **Modifies only**: `crates/jacquard-api/src/`


## String Type Pattern

All validated string types follow a consistent pattern:
- Constructors: `new()`, `new_owned()`, `new_static()`, `raw()`, `unchecked()`, `as_str()`
- Traits: `Serialize`, `Deserialize`, `FromStr`, `Display`, `Debug`, `PartialEq`, `Eq`, `Hash`, `Clone`, conversions to/from `String`/`CowStr`/`SmolStr`, `AsRef<str>`, `Deref<Target=str>`
- Implementation notes: Prefer `#[repr(transparent)]` where possible; use `SmolStr` for short strings, `CowStr` for longer; implement or derive `IntoStatic` for owned conversion
- When constructing from a static string, use `new_static()` to avoid unnecessary allocations

## Lifetimes and Zero-Copy Deserialization

All API types support borrowed deserialization via explicit lifetimes:
- Request/output types: parameterised by `'de` lifetime (e.g., `GetAuthorFeed<'de>`, `GetAuthorFeedOutput<'de>`)
- Fields use `#[serde(borrow)]` where possible (strings, nested objects with lifetimes)
- `CowStr<'a>` enables efficient borrowing from input buffers or owning small strings inline (via `SmolStr`)
- All types implement `IntoStatic` trait to convert borrowed data to owned (`'static`) variants
- Code generator automatically propagates lifetimes through nested structures

Response lifetime handling:
- `Response::parse()` borrows from the response buffer for zero-copy parsing
- `Response::into_output()` converts to owned data using `IntoStatic`
- `Response::transmute()`: reinterpret response as different type (used for typed collection responses)
- Both methods provide typed error handling

## API Coverage (jacquard-api)

**NOTE: jacquard does modules a bit differently in API codegen**
- Specifially, it puts '*.defs' codegen output into the corresponding module file (mod_name.rs in parent directory, NOT mod.rs in module directory)
- It also combines the top-level tld and domain ('com.atproto' -> `com_atproto`, etc.)

## Value Types (jacquard-common)

For working with loosely-typed atproto data:
- `Data<'a>`: Validated, typed representation of atproto values
- `RawData<'a>`: Unvalidated raw values from deserialization
- `from_data`, `from_raw_data`, `to_data`, `to_raw_data`: Convert between typed and untyped
- Useful for second-stage deserialization of `type "unknown"` fields (e.g., `PostView.record`)

Collection types:
- `Collection` trait: Marker trait for record types with `NSID` constant and `Record` associated type
- `RecordError<'a>`: Generic error type for record retrieval operations (RecordNotFound, Unknown)

## Lifetime Design Pattern

Jacquard uses a specific pattern to enable zero-copy deserialization while avoiding HRTB issues and async lifetime problems:

**GATs on associated types** instead of trait-level lifetimes:
```rust
trait XrpcResp {
    type Output<'de>: Deserialize<'de> + IntoStatic;  // GAT, not trait-level lifetime
}
```

**Method-level generic lifetimes** for trait methods that need them:
```rust
fn extract_vec<'s>(output: Self::Output<'s>) -> Vec<Item>
```

**Response wrapper owns buffer** to solve async lifetime issues:
```rust
async fn get_record<R>(&self, rkey: K) -> Result<Response<R>>
// Caller chooses: response.parse() (borrow) or response.into_output() (owned)
```

This pattern avoids `for<'any> Trait<'any>` bounds (which force `DeserializeOwned` semantics) while giving callers control over borrowing vs owning. See `jacquard-common` crate docs for detailed explanation.

## WASM Compatibility

Core crates (`jacquard-common`, `jacquard-api`, `jacquard-identity`, `jacquard-oauth`) support `wasm32-unknown-unknown` target compilation.

Implementation approach:
- **`trait-variant`**: Traits use `#[cfg_attr(not(target_arch = "wasm32"), trait_variant::make(Send))]` to conditionally exclude `Send` bounds on WASM
- **Trait methods with `Self: Sync` bounds**: Duplicated as platform-specific versions (`#[cfg(not(target_arch = "wasm32"))]` vs `#[cfg(target_arch = "wasm32")]`)
- **Helper functions**: Extracted to free functions with platform-specific versions to avoid code duplication
- **Feature gating**: Platform-specific features (e.g., DNS resolution, tokio runtime detection) properly gated behind `cfg` attributes

Test WASM compilation:
```bash
just check-wasm
# or: cargo build --target wasm32-unknown-unknown -p jacquard-common --no-default-features
```

## Client Architecture

### XRPC Request/Response Layer

Core traits:
- `XrpcRequest`: Defines NSID, method (Query/Procedure), and associated Response type
  - `encode_body()` for request serialization (default: JSON; override for CBOR/multipart)
  - `decode_body(&'de [u8])` for request deserialization (server-side)
- `XrpcResp`: Response marker trait with NSID, encoding, Output/Err types
- `XrpcEndpoint`: Server-side trait with PATH, METHOD, and associated Request/Response types
- `XrpcClient`: Stateful trait with `base_uri()`, `opts()`, and `send()` method
  - **This should be your primary interface point with the crate, along with the Agent___ traits**
- `XrpcExt`: Extension trait providing stateless `.xrpc(base)` builder on any `HttpClient`

### Session Management

`Agent<A: AgentSession>` wrapper supports:
- `CredentialSession<S, T>`: App-password (Bearer) authentication with auto-refresh
  - Uses `SessionStore` trait implementers for token persistence (`MemorySessionStore`, `FileAuthStore`)
- `OAuthSession<T, S>`: DPoP-bound OAuth with nonce handling
  - Uses `ClientAuthStore` trait implementers for state/token persistence

Session traits:
- `AgentSession`: common interface for both session types
- `AgentKind`: enum distinguishing AppPassword vs OAuth
- Both sessions implement `HttpClient` and `XrpcClient` for uniform API
- `AgentSessionExt` extension trait includes several helpful methods for atproto record operations.
  - **This trait is implemented automatically for anything that implements both `AgentSession` and `IdentityResolver`**


## Identity Resolution

`JacquardResolver` (default) and custom resolvers implement `IdentityResolver` + `OAuthResolver`:
- Handle → DID: DNS TXT (feature `dns`, or via Cloudflare DoH), HTTPS well-known, PDS XRPC, public fallbacks
- DID → Doc: did:web well-known, PLC directory, PDS XRPC
- OAuth metadata: `.well-known/oauth-protected-resource` and `.well-known/oauth-authorization-server`
- Resolvers use stateless XRPC calls (no auth required for public resolution endpoints)

## Streaming Support

### HTTP Streaming

Feature: `streaming`

Core types in `jacquard-common`:
- `ByteStream` / `ByteSink`: Platform-agnostic stream wrappers (uses n0-future)
- `StreamError`: Concrete error type with Kind enum (Transport, Closed, Protocol)
- `HttpClientExt`: Trait extension for streaming methods
- `StreamingResponse`: XRPC streaming response wrapper

### WebSocket Support

Feature: `websocket` (requires `streaming`)
- `WebSocketClient` trait (independent from `HttpClient`)
- `WebSocketConnection` with tx/rx `ByteSink`/`ByteStream`
- tokio-tungstenite-wasm used to abstract across native + wasm

**Known gaps:**
- Service auth replay protection (jti tracking)
- Video upload helpers (upload + job polling)
- Additional session storage backends (SQLite, etc.)
- PLC operations
- OAuth extractor for Axum
