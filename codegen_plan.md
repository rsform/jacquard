# Lexicon Codegen Plan

## Goal
Generate idiomatic Rust types from AT Protocol lexicon schemas with minimal nesting/indirection.

## Existing Infrastructure

### Already Implemented
- **lexicon.rs**: Complete lexicon parsing types (`LexiconDoc`, `LexUserType`, `LexObject`, etc)
- **fs.rs**: Directory walking for finding `.json` lexicon files
- **schema.rs**: `find_ref_unions()` - collects union fields from a single lexicon
- **output.rs**: Partial - has string type mapping and doc comment generation

### Attribute Macros
- `#[lexicon]` - adds `extra_data` field to structs
- `#[open_union]` - adds `Unknown(Data<'s>)` variant to enums

## Design Decisions

### Module/File Structure
- NSID `app.bsky.feed.post` → `app_bsky/feed/post.rs`
- Flat module names (no `app::bsky`, just `app_bsky`)
- Parent modules: `app_bsky/feed.rs` with `pub mod post;`

### Type Naming
- **Main def**: Use last segment of NSID
  - `app.bsky.feed.post#main` → `Post`
- **Other defs**: Pascal-case the def name
  - `replyRef` → `ReplyRef`
- **Union variants**: Use last segment of ref NSID
  - `app.bsky.embed.images` → `Images`
  - Collisions resolved by module path, not type name
- **No proliferation of `Main` types** like atrium has

### Type Generation

#### Records (lexRecord)
```rust
#[lexicon]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Post<'s> {
    /// Client-declared timestamp...
    pub created_at: Datetime,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embed: Option<RecordEmbed<'s>>,
    pub text: CowStr<'s>,
}
```

#### Objects (lexObject)
Same as records but without `#[lexicon]` if inline/not a top-level def.

#### Unions (lexRefUnion)
```rust
#[open_union]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "$type")]
pub enum RecordEmbed<'s> {
    #[serde(rename = "app.bsky.embed.images")]
    Images(Box<jacquard_api::app_bsky::embed::Images<'s>>),
    #[serde(rename = "app.bsky.embed.video")]
    Video(Box<jacquard_api::app_bsky::embed::Video<'s>>),
}
```

- Use `Box<T>` for all variants (handles circular refs)
- `#[open_union]` adds `Unknown(Data<'s>)` catch-all

#### Queries (lexXrpcQuery)
```rust
pub struct GetAuthorFeedParams<'s> {
    pub actor: AtIdentifier<'s>,
    pub limit: Option<i64>,
    pub cursor: Option<CowStr<'s>>,
}

pub struct GetAuthorFeedOutput<'s> {
    pub cursor: Option<CowStr<'s>>,
    pub feed: Vec<FeedViewPost<'s>>,
}
```

- Flat params/output structs
- No nesting like `Input { params: {...} }`

#### Procedures (lexXrpcProcedure)
Same as queries but with both `Input` and `Output` structs.

### Field Handling

#### Optional Fields
- Fields not in `required: []` → `Option<T>`
- Add `#[serde(skip_serializing_if = "Option::is_none")]`

#### Lifetimes
- All types have `'a` lifetime for borrowing from input
- `#[serde(borrow)]` where needed for zero-copy

#### Type Mapping
- `LexString` with format → specific types (`Datetime`, `Did`, etc)
- `LexString` without format → `CowStr<'a>`
- `LexInteger` → `i64`
- `LexBoolean` → `bool`
- `LexBytes` → `Bytes`
- `LexCidLink` → `CidLink<'a>`
- `LexBlob` → `Blob<'a>`
- `LexRef` → resolve to actual type path
- `LexRefUnion` → generate enum
- `LexArray` → `Vec<T>`
- `LexUnknown` → `Data<'a>`

### Reference Resolution

#### Known Refs
- Check corpus for ref existence
- `#ref: "app.bsky.embed.images"` → `jacquard_api::app_bsky::embed::Images<'a>`
- Handle fragments: `#ref: "com.example.foo#bar"` → `jacquard_api::com_example::foo::Bar<'a>`

#### Unknown Refs
- **In struct fields**: use `Data<'a>` as fallback type
- **In union variants**: handled by `Unknown(Data<'a>)` variant from `#[open_union]`
- Optional: log warnings for missing refs

## Implementation Phases

### Phase 1: Corpus Loading & Registry
**Goal**: Load all lexicons into memory for ref resolution

**Tasks**:
1. Create `LexiconCorpus` struct
   - `HashMap<SmolStr, LexiconDoc<'static>>` - NSID → doc
   - Methods: `load_from_dir()`, `get()`, `resolve_ref()`
2. Load all `.json` files from lexicon directory
3. Parse into `LexiconDoc` and insert into registry
4. Handle fragments in refs (`nsid#def`)

**Output**: Corpus registry that can resolve any ref

### Phase 2: Ref Analysis & Union Collection
**Goal**: Build complete picture of what refs exist and what unions need

**Tasks**:
1. Extend `find_ref_unions()` to work across entire corpus
2. For each union, collect all refs and check existence
3. Build `UnionRegistry`:
   - Union name → list of (known refs, unknown refs)
4. Detect circular refs (optional - or just Box everything)

**Output**: Complete list of unions to generate with their variants

### Phase 3: Code Generation - Core Types
**Goal**: Generate Rust code for individual types

**Tasks**:
1. Implement type generators:
   - `generate_struct()` for records/objects
   - `generate_enum()` for unions
   - `generate_field()` for object properties
   - `generate_type()` for primitives/refs
2. Handle optional fields (`required` list)
3. Add doc comments from `description`
4. Apply `#[lexicon]` / `#[open_union]` macros
5. Add serde attributes

**Output**: `TokenStream` for each type

### Phase 4: Module Organization
**Goal**: Organize generated types into module hierarchy

**Tasks**:
1. Parse NSID into components: `["app", "bsky", "feed", "post"]`
2. Determine file paths: `app_bsky/feed/post.rs`
3. Generate module files: `app_bsky/feed.rs` with `pub mod post;`
4. Generate root module: `app_bsky.rs`
5. Handle re-exports if needed

**Output**: File path → generated code mapping

### Phase 5: File Writing
**Goal**: Write generated code to filesystem

**Tasks**:
1. Format code with `prettyplease`
2. Create directory structure
3. Write module files
4. Write type files
5. Optional: run `rustfmt`

**Output**: Generated code on disk

### Phase 6: Testing & Validation
**Goal**: Ensure generated code compiles and works

**Tasks**:
1. Generate code for test lexicons
2. Compile generated code
3. Test serialization/deserialization
4. Test union variant matching
5. Test extra_data capture

## Edge Cases & Considerations

### Circular References
- **Simple approach**: Union variants always use `Box<T>` → handles all circular refs
- **Alternative**: DFS cycle detection to only Box when needed
  - Track visited refs and recursion stack
  - If ref appears in rec_stack → cycle detected
  - Algorithm:
    ```rust
    fn has_cycle(corpus, start_ref, visited, rec_stack) -> bool {
        visited.insert(start_ref);
        rec_stack.insert(start_ref);

        for child_ref in collect_refs_from_def(resolve(start_ref)) {
            if !visited.contains(child_ref) {
                if has_cycle(corpus, child_ref, visited, rec_stack) {
                    return true;
                }
            } else if rec_stack.contains(child_ref) {
                return true; // back edge = cycle
            }
        }

        rec_stack.remove(start_ref);
        false
    }
    ```
  - Only box variants that participate in cycles
- **Recommendation**: Start with simple (always Box), optimize later if needed

### Name Collisions
- Multiple types with same name in different lexicons
- Module path disambiguates: `app_bsky::feed::Post` vs `com_example::feed::Post`

### Unknown Refs
- Fallback to `Data<'s>` in struct fields
- Caught by `Unknown` variant in unions
- Warn during generation

### Inline Defs
- Nested objects/unions in same lexicon
- Generate as separate types in same file
- Keep names scoped to parent (e.g., `PostReplyRef`)

### Arrays
- `Vec<T>` for arrays
- Handle nested unions in arrays

### Tokens
- Simple marker types
- Generate as unit structs or type aliases?

## Traits for Generated Types

### Collection Trait (Records)
Records implement the existing `Collection` trait from jacquard-common:

```rust
pub struct Post<'a> {
    // ... fields
}

impl Collection for Post<'_> {
    const NSID: &'static str = "app.bsky.feed.post";
    type Record = Post<'static>;
}
```

### XrpcRequest Trait (Queries/Procedures)
New trait for XRPC endpoints:

```rust
pub trait XrpcRequest<'x> {
    /// The NSID for this XRPC method
    const NSID: &'static str;

    /// HTTP method (GET for queries, POST for procedures)
    const METHOD: XrpcMethod;

    /// Input encoding (MIME type, e.g., "application/json")
    /// None for queries (no body)
    const INPUT_ENCODING: Option<&'static str>;

    /// Output encoding (MIME type)
    const OUTPUT_ENCODING: &'static str;

    /// Request parameters type (query params or body)
    type Params: Serialize;

    /// Response output type
    type Output: Deserialize<'x>;
}

pub enum XrpcMethod {
    Query,  // GET
    Procedure, // POST
}
```

**Generated implementation:**
```rust
pub struct GetAuthorFeedParams<'a> {
    pub actor: AtIdentifier<'a>,
    pub limit: Option<i64>,
    pub cursor: Option<CowStr<'a>>,
}

pub struct GetAuthorFeedOutput<'a> {
    pub cursor: Option<CowStr<'a>>,
    pub feed: Vec<FeedViewPost<'a>>,
}

impl XrpcRequest for GetAuthorFeedParams<'_> {
    const NSID: &'static str = "app.bsky.feed.getAuthorFeed";
    const METHOD: XrpcMethod = XrpcMethod::Query;
    const INPUT_ENCODING: Option<&'static str> = None; // queries have no body
    const OUTPUT_ENCODING: &'static str = "application/json";

    type Params = Self;
    type Output = GetAuthorFeedOutput<'static>;
}
```

**Encoding variations:**
- Most procedures: `"application/json"` for input/output
- Blob uploads: `"*/*"` or specific MIME type for input
- CAR files: `"application/vnd.ipld.car"` for repo operations
- Read from lexicon's `input.encoding` and `output.encoding` fields

**Trait benefits:**
- Allows monomorphization (static dispatch) for performance
- Also supports `dyn XrpcRequest` for dynamic dispatch if needed
- Client code can be generic over `impl XrpcRequest`

### Subscriptions
WebSocket streams - defer for now. Will need separate trait with message types.

## Open Questions

1. **Validation**: Generate runtime validation (min/max length, regex, etc)?
2. **Tokens**: How to represent token types?
3. **Errors**: How to handle codegen errors (missing refs, invalid schemas)?
4. **Incremental**: Support incremental codegen (only changed lexicons)?
5. **Formatting**: Always run rustfmt or rely on prettyplease?
6. **XrpcRequest location**: Should trait live in jacquard-common or separate jacquard-xrpc crate?

## Success Criteria

- [ ] Generates code for all official AT Protocol lexicons
- [ ] Generated code compiles without errors
- [ ] No `Main` proliferation
- [ ] Union variants have readable names
- [ ] Unknown refs handled gracefully
- [ ] `#[lexicon]` and `#[open_union]` applied correctly
- [ ] Serialization round-trips correctly
