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

- **jacquard-common**: Core AT Protocol types (DIDs, handles, at-URIs, NSIDs, TIDs, CIDs, etc.) and the `CowStr` type for efficient string handling
- **jacquard-lexicon**: Lexicon parsing and Rust code generation from lexicon schemas
- **jacquard-api**: Generated API bindings (currently empty/in development)
- **jacquard-derive**: Derive macros for lexicon structures
- **jacquard**: Main binary (currently minimal)

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

## Current State & Next Steps

### Completed
- ✅ Comprehensive validation tests for all core string types (handle, DID, NSID, TID, record key, AT-URI, datetime, language, identifier)
- ✅ Validated implementations against AT Protocol specs and TypeScript reference implementation
- ✅ String type interface standardization (Language now has `new_static()`, Datetime has full conversion traits)
- ✅ Data serialization: Full serialize/deserialize for `Data<'_>`, `Array`, `Object` with format-specific handling (JSON vs CBOR)
- ✅ CidLink wrapper type with automatic `{"$link": "cid"}` serialization in JSON
- ✅ Integration test with real Bluesky thread data validates round-trip correctness

### Next Steps
1. **Lexicon Code Generation**: Begin work on lexicon-to-Rust code generation now that core types are stable
