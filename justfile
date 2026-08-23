default:
    @just --list

# Run pre-commit hooks on all files, including autoformatting
pre-commit-all:
    pre-commit run --all-files

# Run tests with default features
test *ARGS:
    cargo nextest run --workspace {{ ARGS }}

publish: test-all check-wasm e2e
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

# Run tests across the scoped feature matrix
test-all: && test-docs test-doctests
    @echo "── default ──"
    cargo nextest run --workspace
    @echo ""
    @echo "── scope-check ──"
    cargo nextest run --workspace --features scope-check
    @echo ""
    @echo "── streaming ──"
    cargo nextest run --workspace --features streaming
    @echo ""
    @echo "── websocket ──"
    cargo nextest run --workspace --features websocket
    @echo ""
    @echo "── scope-check + streaming ──"
    cargo nextest run --workspace --features scope-check,streaming
    @echo ""
    @echo "── scope-check + websocket ──"
    cargo nextest run --workspace --features scope-check,websocket

# Build documentation with the feature sets configured for docs.rs.
test-docs:
    @echo "── docs.rs: jacquard-common ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard-common --no-deps --features crypto-k256,crypto-ed25519,crypto-p256,websocket,zstd,service-auth,reqwest-client,reqwest-stream,crypto
    @echo ""
    @echo "── docs.rs: jacquard-oauth ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard-oauth --no-deps --features loopback,browser-open
    @echo ""
    @echo "── docs.rs: jacquard-repo ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard-repo --no-deps --all-features
    @echo ""
    @echo "── docs.rs: jacquard ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard --no-deps --features api_all,derive,dns,streaming
    @echo ""
    @echo "── docs.rs: tokio-tungstenite-wasm ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p tokio-tungstenite-wasm-jacquard --no-deps --all-features
    @echo ""
    @echo "── docs.rs: jacquard-api ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard-api --no-deps --features bluesky,other,streaming
    @echo ""
    @echo "── docs.rs: remaining workspace crates ──"
    DOCS_RS=1 RUSTDOCFLAGS="--cfg docsrs" cargo doc -p jacquard-identity -p jacquard-lexicon -p jacquard-lexgen -p jacquard-derive -p jacquard-axum -p lazy-collections -p mini-moka-wasm --no-deps

# Run doctests across the scoped feature matrix.
test-doctests:
    @echo "── doctests: default ──"
    cargo test --doc --workspace
    @echo ""
    @echo "── doctests: scope-check ──"
    cargo test --doc --workspace --features scope-check
    @echo ""
    @echo "── doctests: streaming ──"
    cargo test --doc --workspace --features streaming,reqwest-stream
    @echo ""
    @echo "── doctests: websocket ──"
    cargo test --doc --workspace --features websocket
    @echo ""
    @echo "── doctests: scope-check + streaming ──"
    cargo test --doc --workspace --features scope-check,streaming
    @echo ""
    @echo "── doctests: scope-check + websocket ──"
    cargo test --doc --workspace --features scope-check,websocket

# Run tests with a specific feature set
test-feature FEATURE *ARGS:
    cargo nextest run --workspace --features {{ FEATURE }} {{ ARGS }}

# Check that jacquard-common compiles for wasm32
check-wasm:
    cargo build --target wasm32-unknown-unknown -p jacquard-common --features websocket,reqwest-client
    cargo build --target wasm32-unknown-unknown -p jacquard --no-default-features --features api_bluesky,streaming,zstd

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

# Run the full-stack e2e harness against all stable providers (opt-in; never
# part of ordinary `cargo nextest run`). Requires Docker with rootful bridge
# networking. Tranquil needs `docker login atcr.io` first.
e2e:
    nix develop .#e2e -c bash -euc './scripts/e2e.sh tranquil; ./scripts/e2e.sh reference; ./scripts/e2e.sh jetstream'

# Run the e2e harness against one provider: tranquil, reference, or jetstream
e2e-provider PROVIDER *ARGS:
    nix develop .#e2e -c ./scripts/e2e.sh {{ PROVIDER }} {{ ARGS }}

# Show the retained diagnostics bundle for an e2e run
e2e-logs RUN_ID:
    @ls -la target/e2e/{{ RUN_ID }}/ && echo "--- ps.txt ---" && sed -n 1,40p target/e2e/{{ RUN_ID }}/ps.txt 2>/dev/null; \
    for f in target/e2e/{{ RUN_ID }}/*.log; do [ -f "$f" ] && echo "--- $f (tail) ---" && tail -30 "$f"; done
