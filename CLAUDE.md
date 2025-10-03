# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Jacquard is a suite of Rust crates for the AT Protocol (atproto/Bluesky). The project emphasizes spec-compliant, validated, performant baseline types with minimal boilerplate. Key design goals:

- Validated AT Protocol types including typed at:// URIs
- Custom lexicon extension support
- Lexicon `Value` type for working with unknown atproto data (dag-cbor or json)
- Using as much or as little of the crates as needed

## Workspace Structure

This is a Cargo workspace with several crates:

- **jacquard**: Main library crate with XRPC client and public API surface (re-exports jacquard-api and jacquard-common)
- **jacquard-common**: Core AT Protocol types (DIDs, handles, at-URIs, NSIDs, TIDs, CIDs, etc.) and the `CowStr` type for efficient string handling
- **jacquard-lexicon**: Lexicon parsing and Rust code generation from lexicon schemas
- **jacquard-api**: Generated API bindings from lexicon schemas (implementation detail, not directly used by consumers)
- **jacquard-derive**: Attribute macros (`#[lexicon]`, `#[open_union]`) for lexicon structures

## Development Commands

### Using Nix (preferred)
```bash
# Enter dev shell
nix develop

# Build
nix build

# Run
nix develop -c cargo run
```

### Using Cargo/Just
```bash
# Build
cargo build

# Run tests
cargo test

# Run specific test
cargo test <test_name>

# Run specific package tests
cargo test -p <package_name>

# Run
cargo run

# Auto-recompile and run
just watch [ARGS]

# Format and lint all
just pre-commit-all

# Generate API bindings from lexicon schemas
cargo run -p jacquard-lexicon --bin jacquard-codegen -- -i <input_dir> -o <output_dir> [-r <root_module>]
# Example:
cargo run -p jacquard-lexicon --bin jacquard-codegen -- -i crates/jacquard-lexicon/tests/fixtures/lexicons/atproto/lexicons -o crates/jacquard-api/src -r crate
```

## String Type Pattern

The codebase uses a consistent pattern for validated string types. Each type should have:

### Constructors
- `new()`: Construct from a string slice with appropriate lifetime (borrows)
- `new_owned()`: Construct from `impl AsRef<str>`, taking ownership
- `new_static()`: Construct from `&'static str` using `SmolStr`/`CowStr`'s static constructor (no allocation)
- `raw()`: Same as `new()` but panics instead of returning `Result`
- `unchecked()`: Same as `new()` but doesn't validate (marked `unsafe`)
- `as_str()`: Return string slice

### Traits
All string types should implement:
- `Serialize` + `Deserialize` (custom impl for latter, sometimes for former)
- `FromStr`, `Display`
- `Debug`, `PartialEq`, `Eq`, `Hash`, `Clone`
- `From<T> for String`, `CowStr`, `SmolStr`
- `From<String>`, `From<CowStr>`, `From<SmolStr>`, or `TryFrom` if likely to fail
- `AsRef<str>`
- `Deref` with `Target = str` (usually)

### Implementation Details
- Use `#[repr(transparent)]` when possible (exception: at-uri type and components)
- Use `SmolStr` directly as inner type if most instances will be under 24 bytes
- Use `CowStr` for longer strings to allow borrowing from input
- Implement `IntoStatic` trait to take ownership of string types

## Code Style

- Avoid comments for self-documenting code
- Comments should not detail fixes when refactoring
- Professional writing within source code and comments only
- Prioritize long-term maintainability over implementation speed

## Testing

- Write test cases for all critical code
- Tests can be run per-package or workspace-wide
- Use `cargo test <name>` to run specific tests
- Current test coverage: 89 tests in jacquard-common

## Lexicon Code Generation

The `jacquard-codegen` binary generates Rust types from AT Protocol Lexicon schemas:

- Generates structs with `#[lexicon]` attribute for forward compatibility (captures unknown fields in `extra_data`)
- Generates enums with `#[open_union]` attribute for handling unknown variants (unless marked `closed` in lexicon)
- Resolves local refs (e.g., `#image` becomes `Image<'a>`)
- Extracts doc comments from lexicon `description` fields
- Adds header comments with `@generated` marker and lexicon NSID
- Handles XRPC queries, procedures, subscriptions, and errors
- Generates proper module tree with Rust 2018 style
- **XrpcRequest trait**: Implemented directly on params/input structs (not marker types), with GATs for Output<'de> and Err<'de>
- **IntoStatic trait**: All generated types implement `IntoStatic` to convert borrowed types to owned ('static) variants
- **Collection trait**: Implemented on record types directly, with const NSID

## Current State & Next Steps

### Completed
- ✅ Comprehensive validation tests for all core string types (handle, DID, NSID, TID, record key, AT-URI, datetime, language, identifier)
- ✅ Validated implementations against AT Protocol specs and TypeScript reference implementation
- ✅ String type interface standardization (Language now has `new_static()`, Datetime has full conversion traits)
- ✅ Data serialization: Full serialize/deserialize for `Data<'_>`, `Array`, `Object` with format-specific handling (JSON vs CBOR)
- ✅ CidLink wrapper type with automatic `{"$link": "cid"}` serialization in JSON
- ✅ Integration test with real Bluesky thread data validates round-trip correctness
- ✅ Lexicon code generation with forward compatibility and proper lifetime handling
- ✅ IntoStatic implementations for all generated types (structs, enums, unions)
- ✅ XrpcRequest trait with GATs, implemented on params/input types directly
- ✅ HttpClient and XrpcClient traits with generic send_xrpc implementation
- ✅ Response wrapper with parse() (borrowed) and into_output() (owned) methods
- ✅ Structured error types (ClientError, TransportError, EncodeError, DecodeError, HttpError, AuthError)

### Next Steps
1. **Concrete HttpClient Implementation**: Implement HttpClient for reqwest::Client and potentially other HTTP clients
2. **Error Handling Improvements**: Add XRPC error parsing, better HTTP status code handling, structured error responses
3. **Authentication**: Session management, token refresh, DPoP support
4. **Body Encoding**: Support for non-JSON encodings (CBOR, multipart, etc.) in procedures
5. **Lexicon Resolution**: Fetch lexicons from web sources (atproto authorities, git repositories) and parse into corpus
6. **Custom Lexicon Support**: Allow users to plug in their own generated lexicons alongside jacquard-api types in the client/server layer
7. **Public API**: Design the main API surface in `jacquard` that re-exports and wraps generated types
8. **DID Document Support**: Parsing, validation, and resolution of DID documents
9. **OAuth Implementation**: OAuth flow support for authentication
10. **Examples & Documentation**: Create examples and improve documentation
11. **Testing**: Comprehensive tests for generated code and round-trip serialization
