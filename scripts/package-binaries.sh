#!/usr/bin/env bash
set -euo pipefail

# Script to package jacquard-codegen and lex-fetch binaries for distribution
# Creates tar.xz archives with binaries, README, LICENSE, and config files
#
# Generates two versions:
# - Unversioned archives in binaries/ (tracked in git, overwritten each build)
# - Versioned archives in binaries/releases/ (gitignored, for GitHub releases)

# Determine project root (script is in scripts/ subdirectory)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$PROJECT_ROOT"

# Parse version from workspace Cargo.toml
VERSION=$(grep '^version = ' Cargo.toml | head -1 | sed 's/version = "\(.*\)"/\1/')
echo "Packaging version: $VERSION"

# Detect target triple (default to x86_64-unknown-linux-gnu)
TARGET="${CARGO_BUILD_TARGET:-x86_64-unknown-linux-gnu}"
echo "Target: $TARGET"

# Output directories
OUTPUT_DIR="binaries"
RELEASES_DIR="binaries/releases"
mkdir -p "$OUTPUT_DIR"
mkdir -p "$RELEASES_DIR"

# Build binaries in release mode
echo "Building binaries..."
cargo build --release -p jacquard-lexicon --bin jacquard-codegen
cargo build --release -p jacquard-lexicon --bin lex-fetch

# Binary locations
CODEGEN_BIN="target/release/jacquard-codegen"
LEXFETCH_BIN="target/release/lex-fetch"

# Verify binaries exist
if [[ ! -f "$CODEGEN_BIN" ]]; then
    echo "Error: jacquard-codegen binary not found at $CODEGEN_BIN"
    exit 1
fi

if [[ ! -f "$LEXFETCH_BIN" ]]; then
    echo "Error: lex-fetch binary not found at $LEXFETCH_BIN"
    exit 1
fi

# Helper function to package a binary
package_binary() {
    local BIN_NAME=$1
    local BIN_PATH=$2
    local EXTRA_FILES=("${@:3}")  # Additional files beyond README and LICENSE

    echo "Packaging ${BIN_NAME}..."

    # Names for versioned and unversioned archives
    local VERSIONED_NAME="${BIN_NAME}_${TARGET}_v${VERSION}"
    local UNVERSIONED_NAME="${BIN_NAME}_${TARGET}"

    # Create staging directory
    local STAGE_DIR="/tmp/${VERSIONED_NAME}"
    rm -rf "$STAGE_DIR"
    mkdir -p "$STAGE_DIR"

    # Copy files
    cp "$BIN_PATH" "$STAGE_DIR/"
    cp LICENSE "$STAGE_DIR/"
    cp README.md "$STAGE_DIR/"
    for file in "${EXTRA_FILES[@]}"; do
        [[ -n "$file" ]] && cp "$file" "$STAGE_DIR/"
    done

    # Strip binary (reduce size)
    strip "$STAGE_DIR/$BIN_NAME" || echo "Warning: strip failed, skipping"

    # Create versioned archive (for releases)
    cd /tmp
    tar -cJf "${VERSIONED_NAME}.tar.xz" "$VERSIONED_NAME"
    mv "${VERSIONED_NAME}.tar.xz" "$PROJECT_ROOT/$RELEASES_DIR/"
    echo "  Created: ${RELEASES_DIR}/${VERSIONED_NAME}.tar.xz"

    # Rename staging directory for unversioned archive
    mv "$VERSIONED_NAME" "$UNVERSIONED_NAME"

    # Create unversioned archive (tracked in git)
    tar -cJf "${UNVERSIONED_NAME}.tar.xz" "$UNVERSIONED_NAME"
    mv "${UNVERSIONED_NAME}.tar.xz" "$PROJECT_ROOT/$OUTPUT_DIR/"
    echo "  Created: ${OUTPUT_DIR}/${UNVERSIONED_NAME}.tar.xz"

    # Cleanup
    rm -rf "$UNVERSIONED_NAME"
    cd "$PROJECT_ROOT"
}

# Package jacquard-codegen
package_binary "jacquard-codegen" "$CODEGEN_BIN"

# Package lex-fetch (with lexicons.kdl)
package_binary "lex-fetch" "$LEXFETCH_BIN" "lexicons.kdl"

# Print summary
echo ""
echo "Packaging complete!"
echo ""
echo "Tracked archives (binaries/):"
ls -lh "$OUTPUT_DIR"/*.tar.xz
echo ""
echo "Release archives (binaries/releases/):"
ls -lh "$RELEASES_DIR"/*.tar.xz

# Generate checksums for tracked archives
echo ""
echo "Generating checksums for tracked archives..."
cd "$OUTPUT_DIR"
sha256sum *.tar.xz > SHA256SUMS
echo "Checksums written to ${OUTPUT_DIR}/SHA256SUMS"
cat SHA256SUMS

# Generate checksums for release archives
echo ""
echo "Generating checksums for release archives..."
cd "$PROJECT_ROOT/$RELEASES_DIR"
sha256sum *.tar.xz > SHA256SUMS
echo "Checksums written to ${RELEASES_DIR}/SHA256SUMS"
cat SHA256SUMS
