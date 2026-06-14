default:
    @just --list

# Run pre-commit hooks on all files, including autoformatting
pre-commit-all:
    pre-commit run --all-files

# Run tests with default features
test *ARGS:
    cargo nextest run {{ ARGS }}

publish:
    @echo "Running tests..."
    test-all
    check-wasm
    @echo "Publishing..."
    cargo publish -p jacquard-common
    cargo publish -p jacquard-lexicon
    cargo publish -p jacquard-derive
    cargo publish -p jacquard-identity
    cargo publish -p jacquard-lexgen
    cargo publish -p jacquard-api
    cargo publish -p jacquard-repo
    cargo publish -p jacquard-oauth
    cargo publish -p jacquard
    cargo publish -p jacquard-axum

# Run tests across the full feature matrix
test-all:
    @echo "── default ──"
    cargo nextest run
    @echo ""
    @echo "── scope-check ──"
    cargo nextest run --features scope-check
    @echo ""
    @echo "── streaming ──"
    cargo nextest run --features streaming
    @echo ""
    @echo "── websocket ──"
    cargo nextest run --features websocket

# Run tests with a specific feature set
test-feature FEATURE *ARGS:
    cargo nextest run --features {{ FEATURE }} {{ ARGS }}

# Check that jacquard-common compiles for wasm32
check-wasm:
    cargo build --target wasm32-unknown-unknown -p jacquard-common --features websocket,reqwest-client
    cargo build --target wasm32-unknown-unknown -p jacquard --no-default-features --features api_bluesky,streaming

# Run 'cargo run' on the project
run *ARGS:
    cargo run {{ ARGS }}

# Run 'bacon' to run the project (auto-recompiles)
watch *ARGS:
    bacon --job run -- -- {{ ARGS }}

update-api:
    cargo run -p jacquard-lexgen --bin lex-fetch -- -v

generate-api:
    cargo run -p jacquard-lexgen --bin jacquard-codegen -- -i crates/jacquard-api/lexicons -o crates/jacquard-api/src

lex-gen *ARGS:
    cargo run -p jacquard-lexgen --bin lex-fetch -- {{ ARGS }}

lex-fetch *ARGS:
    cargo run -p jacquard-lexgen --bin lex-fetch -- --no-codegen {{ ARGS }}

codegen *ARGS:
    cargo run -p jacquard-lexgen --bin jacquard-codegen -- {{ ARGS }}

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
    if [ -f "examples/{{ NAME }}.rs" ]; then
        cargo run -p jacquard --features=api_bluesky,streaming --example {{ NAME }} -- {{ ARGS }}
    elif cargo metadata --format-version=1 --no-deps | \
         jq -e '.packages[] | select(.name == "jacquard-axum") | .targets[] | select(.kind[] == "example" and .name == "{{ NAME }}")' > /dev/null; then
        cargo run -p jacquard-axum --example {{ NAME }}  -- {{ ARGS }}
    else
        echo "Example '{{ NAME }}' not found."
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
