#!/usr/bin/env bash
set -e

echo "Checking WASM compatibility for streaming support..."

# Check jacquard-common builds for wasm32-unknown-unknown
cargo build -p jacquard-common \
    --target wasm32-unknown-unknown \
    --features streaming \
    --no-default-features

echo "✓ WASM build successful"
