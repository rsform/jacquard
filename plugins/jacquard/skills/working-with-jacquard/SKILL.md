---
name: working-with-jacquard
description: Use when working with Jacquard AT Protocol library for Rust - prevents type parameterization mistakes, string allocation antipatterns, and incorrect BosStr usage patterns
---

# Working with Jacquard

## Overview

Jacquard is a Rust AT Protocol library that uses a borrow-or-share type system for correctness, performance, and ergonomics. All API types are parameterized on `S: BosStr = DefaultStr` — the caller chooses the backing string type, not the library.

**Core principle:** Borrow wherever possible, own when needed. The `DefaultStr` (`SmolStr`) default makes ownership painless, but good code still prefers borrowed deserialization, borrowed function parameters, and borrowed intermediates. Validate at construction time.

**Announce at start:** "I'm using the working-with-jacquard skill to ensure correct BosStr and type usage."

## The BosStr Type System

All API types use a single generic parameter `S: BosStr` instead of lifetimes:

```rust
// Generated API type (e.g. Post)
pub struct Post<S: BosStr = DefaultStr> {
    pub text: S,
    pub created_at: Datetime,
    pub embed: Option<PostEmbed<S>>,
    // ...
}
```

**BosStr implementors** (caller's choice):

| Type | Allocates? | `DeserializeOwned`? | Use when |
|------|-----------|---------------------|----------|
| `&str` | No | No | **Preferred.** Function params, ephemeral/internal values |
| `CowStr<'a>` | Only if needed | No | **Preferred for deser.** Zero-copy from buffers |
| `SmolStr` (= `DefaultStr`) | Inline <=23 bytes, Arc longer | Yes | When data must outlive its borrow source |
| `String` | Yes | Yes | Interop with String-based APIs |

**Key insight:** The `SmolStr` default makes ownership ergonomic — no lifetime ceremony, `DeserializeOwned`, `'static` — but it's a convenience floor, not a ceiling. Prefer borrowed types (`&str`, `CowStr<'_>`) wherever possible and only reach for owned when the data must outlive its source.


## Critical Patterns

### Client traits in scope

Nearly all methods for making API calls are provided via traits, which must be in scope.
There are many extension traits that are auto-implemented for **any** struct meeting the prerequisites.

The most common pair is `IdentityResolver + AgentSession`, which gives access to `AgentSessionExt` with record CRUD helpers.

`use jacquard::prelude::*;` brings the critical ones into scope.

**Rule:** Always import the prelude. If the compiler says a method doesn't exist, you likely need a trait in scope. READ the error output carefully.

### String Type Constructors

All validated types (`Did`, `Handle`, `AtUri`, `Nsid`, `Tid`, `Cid`, etc.) are parameterized on `S: BosStr = DefaultStr`:

```rust
// PREFERRED: SmolStr-backed (inline, no heap for short strings)
let did = Did::new("did:plc:abc123")?;

// BEST: Zero-allocation for string literals
let nsid = Nsid::new_static("com.atproto.repo.getRecord");

// When you already have an owned String
let owned = Did::new_owned(some_string)?;

// AVOID: FromStr always allocates a fresh SmolStr
let did: Did = "did:plc:abc123".parse()?;
```

**Borrowing and conversion:**

```rust
// Cheap borrow — returns Type<&str>
let did_ref: Did<&str> = did.borrow();

// Cross-type conversion
let did_string: Did<String> = did.convert::<String>();
```

**Rule:** Use `new()` for default, `new_static()` for string literals, `new_owned()` when you have a `String`. Avoid `FromStr::parse()` unless performance is irrelevant.

### Function Parameters: Borrow by Default

Function parameters should almost always accept borrowed types. There are three levels of borrowing and you should use the cheapest one that works:

```rust
// BEST: Reference to default-backed type (most flexible for callers)
fn process_did(did: &Did) { /* ... */ }

// GOOD: Borrow-parameterized type (zero-copy inner string)
fn process_did(did: Did<&str>) { /* ... */ }

// MOST BORROWED: Both (reference to borrow-parameterized type)
fn process_did(did: &Did<&str>) { /* ... */ }

// AVOID: Taking ownership when you don't need it
fn process_did(did: Did) { /* ... */ }  // consumes unnecessarily
```

For compound types, same principle — borrow the outer type and/or parameterize with a borrowed backing:

```rust
// GOOD: Borrow the struct, strings are still SmolStr inside
fn process_post(post: &Post) { /* ... */ }

// BETTER: Zero-copy all the way down
fn process_post(post: &Post<CowStr<'_>>) { /* ... */ }

// Generic: Accept any backing type
fn process_post<S: BosStr>(post: &Post<S>) { /* ... */ }
```

**Rule:** Don't take ownership of types unless you need to store or return them. Use `&Type`, `Type<&str>`, or `&Type<&str>` for function parameters.

### Response Parsing: Prefer Borrowed

XRPC responses wrap a `Bytes` buffer. Caller chooses backing type at parse time:

```rust
let response = agent.send(request).await?;

// PREFERRED: Zero-copy borrow from buffer
let output = response.parse::<CowStr<'_>>()?;
// output borrows from response — both must stay in scope
// use this when processing in the same scope (the common case)

// When the borrow can't live long enough:
let output = response.into_output()?;
// output is SmolStr-backed, 'static, DeserializeOwned

// Untyped (when you don't know the schema):
let data = response.parse_data()?;  // Data<CowStr<'_>>
let raw = response.parse_raw()?;    // RawData<'_>
```

**Rule:** Prefer `.parse::<CowStr<'_>>()` and only fall back to `.into_output()` when the compiler tells you the borrow doesn't live long enough. Borrowed types *can* cross async boundaries depending on scope and lifetime inference — don't assume async means owned.

**When you genuinely need `DeserializeOwned`:** Some frameworks require it structurally — axum extractors (e.g. `Json<T>`) call `serde_json::from_reader` internally which requires `DeserializeOwned`, and dioxus `use_server_future()` closure arguments have the same constraint. In these cases, use `SmolStr`-backed types (the default). These are the situations the default exists for.

### IntoStatic Trait

Converts any `BosStr`-backed type to its `SmolStr` (owned, `'static`) equivalent:

```rust
use jacquard::common::IntoStatic;

// Convert a CowStr-backed (borrowed) type to owned
let borrowed = response.parse::<CowStr<'_>>()?;
let owned: Post<SmolStr> = borrowed.into_static();

// Also works for validated string types
let did_ref: Did<&str> = did.borrow();
let did_owned: Did<SmolStr> = did_ref.into_static();
```

**When you need it:**
- Converting borrowed (`CowStr<'_>` or `&str`) types to owned for storage or return
- Custom types that use non-default backing strings

**When you don't need it:**
- `SmolStr`-backed types are already `'static` — no conversion needed
- Using `.into_output()` already gives you `SmolStr`-backed types

**Rule:** All custom types with `S: BosStr` parameter MUST derive `IntoStatic`. But for typical usage with default `SmolStr` backing, you rarely call it directly.

### Data vs serde_json::Value

**Never use `serde_json::Value` with Jacquard.**

```rust
// NEVER
let value: serde_json::Value = serde_json::from_slice(bytes)?;

// ALWAYS use Data<S> (owned) or RawData<'a> (zero-copy)
use jacquard::common::{Data, from_data, to_data};

let data: Data = serde_json::from_slice(bytes)?;  // Data<SmolStr>
let post: Post = from_data(&data)?;

// Convert typed -> untyped -> typed
let post = Post::builder().text("test").build();
let data: Data = to_data(&post)?;
let post2: Post = from_data(&data)?;
```

**Two value types:**
- `Data<S: BosStr>`: Typed, owned by default. Use for storage, manipulation, serialization.
- `RawData<'a>`: Lifetime-based, zero-copy. Use for transient parsing from buffers.

**Path access on Data:**
```rust
let data: Data = /* ... */;

// Path syntax: field.nested, [0] for arrays
if let Some(alt) = data.get_at_path("embed.images[0].alt") {
    println!("{}", alt.as_str().unwrap());
}

// Query syntax: [..] wildcard, ..field scoped recursion, ...field global recursion
let alts = data.query("embed.[..].alt");
let handle = data.query("post..handle");       // finds post.author.handle
let all_cids = data.query("...cid");           // all CIDs anywhere
```

Path access has `_mut` and `set_at` equivalents for mutation.

**Rule:** `Data<S>` is Jacquard's replacement for `serde_json::Value`. Always use it for untyped AT Protocol values.


## Common Mistakes

### Using Lifetimes Instead of BosStr on Custom Types

```rust
// WRONG: Old lifetime-based pattern
struct MyOutput<'a> {
    #[serde(borrow)]
    field: CowStr<'a>,
}

// CORRECT: BosStr-parameterized
#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
struct MyOutput<S: BosStr = DefaultStr> {
    field: S,
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    extra_data: Option<BTreeMap<SmolStr, Data<S>>>,
}
```

**Why:** Jacquard types no longer use lifetime parameters. Using `<'a>` instead of `<S: BosStr>` means your type can't compose with Jacquard's API types or response parsing.

### Forgetting the Serde Bound

```rust
// WRONG: Missing serde bound — deserialization will fail
#[derive(Serialize, Deserialize)]
struct MyType<S: BosStr = DefaultStr> {
    name: S,
}

// CORRECT: Explicit serde bound for BosStr
#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
struct MyType<S: BosStr = DefaultStr> {
    name: S,
}
```

**Why:** Serde can't infer the correct bounds for generic types. Without the explicit bound, the derive generates `S: Deserialize<'de>` which is insufficient.

### Roundtripping Through String

```rust
// BAD: Pointless allocation
let did_str = did.as_str().to_string();
let did2 = Did::new(&did_str)?;

// GOOD: Clone, borrow, or convert
let did2 = did.clone();
let did_ref = did.borrow();          // Did<&str>, zero-cost
let did_string = did.convert::<String>();  // cross-type
```

### Dropping Response While Holding Borrowed Parse

```rust
// WILL NOT COMPILE
let output = {
    let response = agent.send(request).await?;
    response.parse::<CowStr<'_>>()?  // borrows from response
};  // response dropped!

// CORRECT: Keep response alive
let response = agent.send(request).await?;
let output = response.parse::<CowStr<'_>>()?;

// OR: Use into_output() for owned types (usually better)
let output = agent.send(request).await?.into_output()?;
```

### Calling .into_static() When You Don't Need To

```rust
// WASTEFUL: into_output() already gives SmolStr-backed types
let response = agent.send(request).await?;
let output = response.into_output()?;
let owned = output.into_static();  // redundant! already SmolStr

// CORRECT: into_output() is sufficient
let output = agent.send(request).await?.into_output()?;
// output is already SmolStr-backed and 'static
```

### Not Deriving IntoStatic on Custom Types

```rust
// WILL NOT COMPILE with zero-copy parsing
#[derive(Serialize, Deserialize)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
struct MyOutput<S: BosStr = DefaultStr> {
    field: S,
}

// REQUIRED: Derive IntoStatic
#[derive(Serialize, Deserialize, IntoStatic)]
#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]
struct MyOutput<S: BosStr = DefaultStr> {
    field: S,
}
```

### Forgetting MST Immutability

MST (Merkle Search Tree) is immutable and persistent:

```rust
// WRONG: Loses result
mst.add(key, cid).await?;

// CORRECT: Reassign
let mst = mst.add(key, cid).await?;
```

### Skipping Issuer Verification in OAuth

```rust
// SECURITY VULNERABILITY
let token_response = exchange_code(...).await?;
// Immediately trusting token_response.sub without verification!

// ALWAYS VERIFY
let token_response = exchange_code(...).await?;
let pds = resolver.verify_issuer(&server_metadata, &token_response.sub).await?;
```

### Non-Exhaustive Union Matches

Open unions have `Unknown(Data<S>)` variant:

```rust
// WILL NOT COMPILE (missing Unknown variant)
match embed {
    PostEmbed::Images(img) => { /* ... */ }
    PostEmbed::Video(vid) => { /* ... */ }
}

// HANDLE ALL VARIANTS
match embed {
    PostEmbed::Images(img) => { /* ... */ }
    PostEmbed::Video(vid) => { /* ... */ }
    _ => { /* Unknown or other variants */ }
}
```

### Forgetting extra_data Field

When constructing types without builders:

```rust
// WRONG: Missing extra_data
let record = MyRecord {
    known_field: value,
};

// CORRECT: Include extra_data
let record = MyRecord {
    known_field: value,
    extra_data: None,
};

// BETTER: Use builder (handles it automatically)
let record = MyRecord::builder()
    .known_field(value)
    .build();
```


## Quick Reference

### String Type Constructors

| Method | Allocates? | Use When |
|--------|-----------|----------|
| `new(s)` | SmolStr (inline <=23b) | Default construction |
| `new_static(&'static str)` | No | String literals |
| `new_owned(String)` | Reuses buffer | Already have a String |
| `.borrow()` | No | Cheap `Type<&str>` reference |
| `.convert::<B>()` | Depends on B | Cross-type conversion |
| `FromStr::parse()` | **Always** | **Avoid** |

### Response Parsing

| Method | Backing | Allocates? | Use When |
|--------|---------|-----------|----------|
| `.parse::<CowStr<'_>>()` | `CowStr<'_>` | No (zero-copy) | **Preferred.** Processing in same scope |
| `.parse_data()` | `Data<CowStr<'_>>` | No | Untyped, zero-copy |
| `.parse_raw()` | `RawData<'_>` | No | Raw untyped, zero-copy |
| `.into_output()` | `SmolStr` | Yes (inline) | When borrow can't live long enough |

### BosStr Backing Types

| Type | Owned? | `DeserializeOwned`? | Typical Use |
|------|--------|---------------------|-------------|
| `SmolStr` (default) | Yes | Yes | When borrow can't live long enough |
| `&str` | No | No | Function params, ephemeral |
| `CowStr<'a>` | Borrow-or-own | No | Zero-copy from buffers |
| `String` | Yes | Yes | Interop |

### Data Types

| Use | Don't Use |
|-----|-----------|
| `Data<S>` (owned) | `serde_json::Value` |
| `RawData<'a>` (zero-copy) | `serde_json::Value` |
| `from_data()` | `serde_json::from_value()` |
| `to_data()` | `serde_json::to_value()` |


## Red Flags

**CATASTROPHIC (stop immediately):**
- Using `serde_json::Value` instead of `Data`
- Skipping OAuth issuer verification
- Using lifetime parameters (`<'a>`) on types that should use `<S: BosStr>`

**WRONG PATTERN (rewrite):**
- Taking ownership (`Did`, `Post`) in function parameters instead of borrowing (`&Did`, `Did<&str>`, `&Post<CowStr<'_>>`)
- Using `.into_output()` when `.parse::<CowStr<'_>>()` would suffice (data processed in same scope)
- Using `#[serde(borrow)]` on BosStr-parameterized fields
- Missing `#[serde(bound(deserialize = "S: Deserialize<'de> + BosStr"))]` on custom types
- Calling `.into_static()` on types that are already `SmolStr`-backed
- Using `FromStr::parse()` on validated types
- Roundtripping through `String` or `to_string()`

**WILL NOT COMPILE:**
- Custom types with `S: BosStr` missing `IntoStatic` derive
- Dropping response while holding borrowed `CowStr<'_>` parse
- Missing `Unknown` variant in union matches
- Forgetting `extra_data` field in manual construction

**SECURITY:**
- Not verifying issuer in OAuth flows
- Not validating DID document ID matches request
- Reusing DPoP proofs across requests
- Not implementing JTI tracking in server code


## Documentation

**Always read the docs before implementing:**
- [jacquard](https://docs.rs/jacquard/latest/jacquard/)
- [jacquard-common](https://docs.rs/jacquard-common/latest/jacquard_common/)
- [jacquard-api](https://docs.rs/jacquard-api/latest/jacquard_api/)
- [jacquard-oauth](https://docs.rs/jacquard-oauth/latest/jacquard_oauth/)
- [jacquard-identity](https://docs.rs/jacquard-identity/latest/jacquard_identity/)

**LLMs.txt for comprehensive patterns:**
- https://tangled.org/@nonbinary.computer/jacquard/raw/main/llms.txt


## Example Patterns

### Making an XRPC Call

```rust
use jacquard::prelude::*;
use jacquard::api::app_bsky::feed::get_author_feed::GetAuthorFeed;

let request = GetAuthorFeed::new()
    .actor("alice.bsky.social".into())
    .limit(50)
    .build();

let response = agent.send(request).await?;
let output = response.into_output()?;  // SmolStr-backed, owned

for item in output.feed {
    println!("{}", item.post.author.handle);
}
```

### Creating a Record

```rust
use jacquard::prelude::*;
use jacquard::api::app_bsky::feed::post::Post;

let post = Post::builder()
    .text("Hello ATProto!")
    .created_at(Datetime::now())
    .build();

agent.create_record(post, None).await?;
```

### Identity Resolution

```rust
use jacquard::identity::PublicResolver;

let resolver = PublicResolver::default();

// Handle -> DID
let did = resolver.resolve_handle(&handle).await?;

// DID -> PDS endpoint
let pds = resolver.pds_for_did(&did).await?;
```

### Custom Type with BosStr

```rust
use jacquard::common::types::BosStr;
use jacquard::common::types::value::Data;
use jacquard::common::DefaultStr;
use jacquard_derive::IntoStatic;
use serde::{Serialize, Deserialize};
use smol_str::SmolStr;
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, IntoStatic, Debug, Clone)]
#[serde(
    rename_all = "camelCase",
    bound(deserialize = "S: Deserialize<'de> + BosStr"),
)]
struct MyRecord<S: BosStr = DefaultStr> {
    name: S,
    count: u32,
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    extra_data: Option<BTreeMap<SmolStr, Data<S>>>,
}
```

### Zero-Copy Response Processing

```rust
use jacquard::common::CowStr;

let response = agent.send(request).await?;

// Zero-copy: borrow strings directly from response buffer
let output = response.parse::<CowStr<'_>>()?;
process_immediately(&output);
// response and output dropped together — no allocations

// If you need to keep it: convert to owned
let owned = output.into_static();
```


## Philosophy

**Jacquard is designed for correctness, performance, and ergonomics — in that order:**

1. **Borrow first, own when needed** — Use `&str`, `CowStr<'_>`, and references by default. Own (`SmolStr`) only when data must outlive its source.
2. **The default is a floor, not a ceiling** — `SmolStr` (= `DefaultStr`) makes ownership painless, but good code still prefers borrowed deserialization and borrowed parameters.
3. **Validation at construction** — Invalid inputs fail fast at `new()`, not deep in application logic.
4. **Caller chooses backing type** — The `S: BosStr` parameter lets you pick the right trade-off per call site.
5. **Batteries included, but replaceable** — High-level `Agent` for convenience, low-level primitives for control.

**When in doubt:** Borrow. Use `.parse::<CowStr<'_>>()`, pass `&Did` or `Did<&str>`, and only reach for owned types when the compiler tells you the data needs to live longer.
