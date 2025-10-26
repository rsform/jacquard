# Schema Discovery Approaches

Jacquard provides two complementary approaches for discovering lexicon schemas from Rust types:

## 1. Inventory-Based Discovery (Link-Time)

**Module:** `schema_extraction`

Uses the `inventory` crate to collect schema types at link time.

### Pros
- ✅ Fast - schemas already in memory
- ✅ Works with compiled dependencies
- ✅ No parsing overhead
- ✅ Guaranteed to match compiled code

### Cons
- ❌ Only discovers types that are **linked** into the binary
- ❌ Requires creating a custom binary that imports your types
- ❌ Won't see unused types that the linker removes

### Usage

```rust
// bin/extract_schemas.rs
use jacquard_lexgen::schema_extraction;
use my_app::models::*;  // ← Must import to link

fn main() -> miette::Result<()> {
    schema_extraction::run("lexicons", true)
}
```

### Best For
- Extracting schemas from your own crate
- When you already have types imported/used
- Production builds where you want to match exactly what's compiled

## 2. Workspace Discovery (Source Scanning)

**Module:** `schema_discovery`

Parses workspace source files directly using `syn`.

### Pros
- ✅ Discovers **all** types in workspace
- ✅ No linking required
- ✅ Works across workspace members
- ✅ Sees types even if they're not used

### Cons
- ❌ Slower - parses all .rs files
- ❌ Doesn't work with binary dependencies
- ❌ Must re-parse source on every run

### Usage

```rust
use jacquard_lexgen::schema_discovery::WorkspaceDiscovery;

fn main() -> miette::Result<()> {
    let schemas = WorkspaceDiscovery::new()
        .verbose(true)
        .scan()?;

    for schema in schemas {
        println!("{}: {}", schema.nsid, schema.type_name);
    }

    Ok(())
}
```

### Best For
- Workspace-wide schema auditing
- Finding all schema types regardless of usage
- Development workflows where you want comprehensive discovery
- When you don't want to maintain import lists

## Comparison

| Feature | Inventory | Workspace Scan |
|---------|-----------|----------------|
| Speed | Fast (runtime) | Slower (parsing) |
| Coverage | Linked types only | All types in workspace |
| Binary deps | ✅ Yes | ❌ No |
| Unused types | ❌ No | ✅ Yes |
| Workspace-wide | ❌ No | ✅ Yes |
| Setup complexity | Medium (need imports) | Low (just run) |

## Hybrid Approach

For best results, use both:

1. **Development:** Use workspace scan for comprehensive discovery
2. **CI/Production:** Use inventory for fast, exact extraction

```bash
# Development: find all schemas
cargo run --example workspace_discovery

# Production: extract linked schemas
cargo run --bin extract-schemas
```

## Future: Schema Generation

Phase 3 currently only **discovers** schemas. A future enhancement could combine
workspace discovery with the derive macro's schema generation logic to actually
**generate** lexicon JSON without needing to link anything.

This would require:
- Extracting schema generation logic from the derive macro
- Calling it directly from the scanner
- Managing dependencies between schema types

Tracked in: [Issue #TBD]
