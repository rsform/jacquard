#!/usr/bin/env bash
set -euo pipefail

# Script to package jacquard-lexicon binaries for distribution using Nix cross-compilation
# Creates tar.xz archives with binaries, man pages, completions, README, LICENSE, and config
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

# Detect current system
CURRENT_SYSTEM=$(nix eval --impure --expr 'builtins.currentSystem' --raw)
echo "Current system: $CURRENT_SYSTEM"

# Map target triples to nix package names and friendly names
declare -A TARGET_TO_PACKAGE=(
    ["x86_64-unknown-linux-gnu"]="jacquard-lexicon-x86_64-linux"
    ["aarch64-unknown-linux-gnu"]="jacquard-lexicon-aarch64-linux"
    ["x86_64-apple-darwin"]="jacquard-lexicon-x86_64-darwin"
    ["aarch64-apple-darwin"]="jacquard-lexicon-aarch64-darwin"
    ["x86_64-pc-windows-gnu"]="jacquard-lexicon-x86_64-windows"
    ["aarch64-pc-windows-gnullvm"]="jacquard-lexicon-aarch64-windows"
)

# Determine which targets we can build from the current system
TARGETS=()
case "$CURRENT_SYSTEM" in
    x86_64-linux)
        # Linux can cross-compile to everything
        TARGETS=(
            "x86_64-unknown-linux-gnu"
            "aarch64-unknown-linux-gnu"
            "x86_64-apple-darwin"
            "aarch64-apple-darwin"
            "x86_64-pc-windows-gnu"
            "aarch64-pc-windows-gnullvm"
        )
        echo "Building from x86_64-linux: All targets (Linux, macOS, Windows for x86_64 and aarch64)"
        ;;
    aarch64-linux)
        # Linux can cross-compile to everything
        TARGETS=(
            "aarch64-unknown-linux-gnu"
            "x86_64-unknown-linux-gnu"
            "x86_64-apple-darwin"
            "aarch64-apple-darwin"
            "x86_64-pc-windows-gnu"
            "aarch64-pc-windows-gnullvm"
        )
        echo "Building from aarch64-linux: All targets (Linux, macOS, Windows for x86_64 and aarch64)"
        ;;
    x86_64-darwin)
        # macOS cross-compilation is limited
        TARGETS=(
            "x86_64-apple-darwin"
        )
        echo "Building from x86_64-darwin: x86_64-darwin only"
        echo "Note: Cross to aarch64-darwin needs rosetta, cross to Linux/Windows needs more setup"
        ;;
    aarch64-darwin)
        # macOS aarch64 can build both macOS targets via rosetta
        TARGETS=(
            "aarch64-apple-darwin"
            "x86_64-apple-darwin"
        )
        echo "Building from aarch64-darwin: macOS targets (aarch64 + x86_64 via rosetta)"
        echo "Note: Cross to Linux/Windows needs more setup"
        ;;
    *)
        echo "Error: Unknown system: $CURRENT_SYSTEM"
        echo "This script supports: x86_64-linux, aarch64-linux, x86_64-darwin, aarch64-darwin"
        exit 1
        ;;
esac

echo ""
echo "Will build for: ${TARGETS[*]}"
echo ""

# Output directories
OUTPUT_DIR="binaries"
RELEASES_DIR="binaries/releases"
mkdir -p "$OUTPUT_DIR"
mkdir -p "$RELEASES_DIR"

# Helper function to package for a target
package_target() {
    local TARGET=$1
    local PACKAGE_NAME="${TARGET_TO_PACKAGE[$TARGET]}"

    echo ""
    echo "======================================"
    echo "Building for $TARGET"
    echo "======================================"

    # Build with nix using cross-compilation package
    echo "Running: nix build .#${PACKAGE_NAME}"
    if ! nix build ".#${PACKAGE_NAME}" -o "result-${TARGET}"; then
        echo "Error: nix build failed for $TARGET"
        return 1
    fi

    # Names for versioned and unversioned archives
    local VERSIONED_NAME="jacquard-lexicon_${TARGET}_v${VERSION}"
    local UNVERSIONED_NAME="jacquard-lexicon_${TARGET}"

    # Create staging directory
    local STAGE_DIR="/tmp/${VERSIONED_NAME}"
    rm -rf "$STAGE_DIR"
    mkdir -p "$STAGE_DIR"

    # Detect if this is a Windows target
    if [[ "$TARGET" == *"windows"* ]]; then
        # Windows: just binaries, README, LICENSE, example config
        mkdir -p "$STAGE_DIR/bin"
        cp "result-${TARGET}"/bin/*.exe "$STAGE_DIR/bin/" 2>/dev/null || true
        cp LICENSE "$STAGE_DIR/"
        cp README.md "$STAGE_DIR/"

        # Copy example config to a more Windows-friendly location
        mkdir -p "$STAGE_DIR/examples"
        cp crates/jacquard-lexicon/lexicons.kdl.example "$STAGE_DIR/examples/" 2>/dev/null || true
    else
        # Unix (Linux/macOS): full structure with man pages and completions
        cp -r "result-${TARGET}"/* "$STAGE_DIR/"
        cp LICENSE "$STAGE_DIR/"
        cp README.md "$STAGE_DIR/"
    fi

    # Create versioned archive (for releases)
    cd /tmp

    # Use .zip for Windows, .tar.xz for Unix
    if [[ "$TARGET" == *"windows"* ]]; then
        zip -r "${VERSIONED_NAME}.zip" "$VERSIONED_NAME"
        mv "${VERSIONED_NAME}.zip" "$PROJECT_ROOT/$RELEASES_DIR/"
        echo "  Created: ${RELEASES_DIR}/${VERSIONED_NAME}.zip"

        # Rename and create unversioned archive
        mv "$VERSIONED_NAME" "$UNVERSIONED_NAME"
        zip -r "${UNVERSIONED_NAME}.zip" "$UNVERSIONED_NAME"
        mv "${UNVERSIONED_NAME}.zip" "$PROJECT_ROOT/$OUTPUT_DIR/"
        echo "  Created: ${OUTPUT_DIR}/${UNVERSIONED_NAME}.zip"
    else
        tar -cJf "${VERSIONED_NAME}.tar.xz" "$VERSIONED_NAME"
        mv "${VERSIONED_NAME}.tar.xz" "$PROJECT_ROOT/$RELEASES_DIR/"
        echo "  Created: ${RELEASES_DIR}/${VERSIONED_NAME}.tar.xz"

        # Rename and create unversioned archive
        mv "$VERSIONED_NAME" "$UNVERSIONED_NAME"
        tar -cJf "${UNVERSIONED_NAME}.tar.xz" "$UNVERSIONED_NAME"
        mv "${UNVERSIONED_NAME}.tar.xz" "$PROJECT_ROOT/$OUTPUT_DIR/"
        echo "  Created: ${OUTPUT_DIR}/${UNVERSIONED_NAME}.tar.xz"
    fi

    # Cleanup
    rm -rf "$UNVERSIONED_NAME"
    rm -f "$PROJECT_ROOT/result-${TARGET}"
    cd "$PROJECT_ROOT"
}

# Build for all targets
for target in "${TARGETS[@]}"; do
    package_target "$target" || echo "Warning: build failed for $target, continuing..."
done

# Print summary
echo ""
echo "Packaging complete!"
echo ""
echo "Tracked archives (binaries/):"
ls -lh "$OUTPUT_DIR"/*.tar.xz 2>/dev/null || true
ls -lh "$OUTPUT_DIR"/*.zip 2>/dev/null || true
echo ""
echo "Release archives (binaries/releases/):"
ls -lh "$RELEASES_DIR"/*.tar.xz 2>/dev/null || true
ls -lh "$RELEASES_DIR"/*.zip 2>/dev/null || true

# Generate checksums for tracked archives
echo ""
echo "Generating checksums for tracked archives..."
cd "$OUTPUT_DIR"
sha256sum *.tar.xz *.zip 2>/dev/null > SHA256SUMS || true
echo "Checksums written to ${OUTPUT_DIR}/SHA256SUMS"
cat SHA256SUMS

# Generate checksums for release archives
echo ""
echo "Generating checksums for release archives..."
cd "$PROJECT_ROOT/$RELEASES_DIR"
sha256sum *.tar.xz *.zip 2>/dev/null > SHA256SUMS || true
echo "Checksums written to ${RELEASES_DIR}/SHA256SUMS"
cat SHA256SUMS
