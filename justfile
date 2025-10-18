default:
    @just --list

# Run pre-commit hooks on all files, including autoformatting
pre-commit-all:
    pre-commit run --all-files

# Check that jacquard-common compiles for wasm32
check-wasm:
    cargo build --target wasm32-unknown-unknown -p jacquard-common --no-default-features

# Run 'cargo run' on the project
run *ARGS:
    cargo run {{ARGS}}

# Run 'bacon' to run the project (auto-recompiles)
watch *ARGS:
	bacon --job run -- -- {{ ARGS }}

update-api:
    cargo run -p jacquard-lexicon --bin lex-fetch -- -v

generate-api:
    cargo run -p jacquard-lexicon --bin jacquard-codegen -- -i crates/jacquard-api/lexicons -o crates/jacquard-api/src -r crate

lex-gen *ARGS:
    cargo run -p jacquard-lexicon --bin lex-fetch -- {{ARGS}}

lex-fetch *ARGS:
    cargo run -p jacquard-lexicon --bin lex-fetch -- --no-codegen {{ARGS}}

codegen *ARGS:
    cargo run -p jacquard-lexicon --bin jacquard-codegen -- -r crate {{ARGS}}

# Package binaries for distribution (creates tar.xz archives)
package-binaries:
    ./scripts/package-binaries.sh

# List all available examples
examples:
    #!/usr/bin/env bash
    echo "jacquard examples:"
    for file in "examples"/*.rs; do
        name=$(basename "$file" .rs)
        echo "  - $name"
    done
    echo ""
    echo "jacquard-axum examples:"
    cargo metadata --format-version=1 --no-deps | \
        jq -r '.packages[] | select(.name == "jacquard-axum") | .targets[] | select(.kind[] == "example") | .name' | \
        sed 's/^/  - /'
    echo ""
    echo "Usage: just example <name> [ARGS...]"

# Run an example by name (auto-detects package)
example NAME *ARGS:
    #!/usr/bin/env bash
    if [ -f "examples/{{NAME}}.rs" ]; then
        cargo run -p jacquard --features=api_bluesky,streaming --example {{NAME}} -- {{ARGS}}
    elif cargo metadata --format-version=1 --no-deps | \
         jq -e '.packages[] | select(.name == "jacquard-axum") | .targets[] | select(.kind[] == "example" and .name == "{{NAME}}")' > /dev/null; then
        cargo run -p jacquard-axum --example {{NAME}}  -- {{ARGS}}
    else
        echo "Example '{{NAME}}' not found."
        echo ""
        echo "jacquard examples:"
        for file in "examples"/*.rs; do
            name=$(basename "$file" .rs)
            echo "  - $name"
        done
        echo ""
        echo "jacquard-axum examples:"
        cargo metadata --format-version=1 --no-deps | \
            jq -r '.packages[] | select(.name == "jacquard-axum") | .targets[] | select(.kind[] == "example") | .name' | \
            sed 's/^/  - /'
        exit 1
    fi
