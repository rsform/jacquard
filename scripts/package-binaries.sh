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

# Map target triples to nix package names
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
        # Linux can cross-compile to other Linux archs and Windows x86_64
        # macOS requires remote builders or actual macOS hardware
        # aarch64-windows: nixpkgs mingw-w64-pthreads broken for aarch64 (missing winver.h)
        TARGETS=(
            "x86_64-unknown-linux-gnu"
            "aarch64-unknown-linux-gnu"
            "x86_64-pc-windows-gnu"
            # "aarch64-pc-windows-gnullvm"  # TODO: nixpkgs cross-compile broken
        )
        echo "Building from x86_64-linux: Linux (x86_64, aarch64) + Windows (x86_64)"
        echo "Note: macOS cross-compilation requires remote builders or macOS hardware"
        echo "Note: aarch64-windows cross-compilation broken in nixpkgs (mingw-w64-pthreads build fails)"
        ;;
    aarch64-linux)
        # Linux can cross-compile to other Linux archs and Windows x86_64
        # macOS requires remote builders or actual macOS hardware
        # aarch64-windows: nixpkgs mingw-w64-pthreads broken for aarch64 (missing winver.h)
        TARGETS=(
            "aarch64-unknown-linux-gnu"
            "x86_64-unknown-linux-gnu"
            "x86_64-pc-windows-gnu"
            # "aarch64-pc-windows-gnullvm"  # TODO: nixpkgs cross-compile broken
        )
        echo "Building from aarch64-linux: Linux (aarch64, x86_64) + Windows (x86_64)"
        echo "Note: macOS cross-compilation requires remote builders or macOS hardware"
        echo "Note: aarch64-windows cross-compilation broken in nixpkgs (mingw-w64-pthreads build fails)"
        ;;
    x86_64-darwin)
        # macOS cross-compilation is limited
        TARGETS=(
            "x86_64-apple-darwin"
        )
        echo "Building from x86_64-darwin: x86_64-darwin only"
        ;;
    aarch64-darwin)
        # macOS aarch64 can build both macOS targets via rosetta
        TARGETS=(
            "aarch64-apple-darwin"
            "x86_64-apple-darwin"
        )
        echo "Building from aarch64-darwin: macOS targets (aarch64 + x86_64 via rosetta)"
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

    # Package each binary separately
    for BINARY in lex-fetch jacquard-codegen; do
        echo "  Packaging binary: $BINARY"
        package_binary "$TARGET" "$BINARY"
    done

    # Cleanup
    rm -f "$PROJECT_ROOT/result-${TARGET}"
    cd "$PROJECT_ROOT"
}

# Helper function to package a single binary
package_binary() {
    local TARGET=$1
    local BINARY=$2

    # Determine binary extension
    local BINARY_EXT=""
    if [[ "$TARGET" == *"windows"* ]]; then
        BINARY_EXT=".exe"
    fi

    # Check if binary exists
    if [[ ! -f "result-${TARGET}/bin/${BINARY}${BINARY_EXT}" ]]; then
        echo "  Warning: ${BINARY}${BINARY_EXT} not found, skipping"
        return 0
    fi

    # Names for versioned and unversioned archives
    local VERSIONED_NAME="${BINARY}_${TARGET}_v${VERSION}"
    local UNVERSIONED_NAME="${BINARY}_${TARGET}"

    # Create staging directory
    local STAGE_DIR="/tmp/${VERSIONED_NAME}"
    rm -rf "$STAGE_DIR"
    mkdir -p "$STAGE_DIR"

    # Detect if this is a Windows target
    if [[ "$TARGET" == *"windows"* ]]; then
        # Windows: binary, README, LICENSE, example config (for lex-fetch only)
        cp "result-${TARGET}/bin/${BINARY}.exe" "$STAGE_DIR/"
        cp LICENSE "$STAGE_DIR/"
        cp README.md "$STAGE_DIR/"

        # Only include example config for lex-fetch
        if [[ "$BINARY" == "lex-fetch" ]]; then
            mkdir -p "$STAGE_DIR/examples"
            cp crates/jacquard-lexicon/lexicons.kdl.example "$STAGE_DIR/examples/" 2>/dev/null || true
        fi
    else
        # Unix (Linux/macOS): binary, man page, completions, README, LICENSE
        mkdir -p "$STAGE_DIR/bin"
        cp "result-${TARGET}/bin/${BINARY}" "$STAGE_DIR/bin/"

        cp LICENSE "$STAGE_DIR/"
        cp README.md "$STAGE_DIR/"

        # Copy man page if it exists
        if [[ -f "result-${TARGET}/share/man/man1/${BINARY}.1.gz" ]]; then
            mkdir -p "$STAGE_DIR/share/man/man1"
            cp "result-${TARGET}/share/man/man1/${BINARY}.1.gz" "$STAGE_DIR/share/man/man1/"
        fi

        # Copy completions if they exist
        for shell_dir in bash fish zsh; do
            local comp_dir="result-${TARGET}/share/$shell_dir/site-functions"
            if [[ "$shell_dir" == "bash" ]]; then
                comp_dir="result-${TARGET}/share/bash-completion/completions"
            elif [[ "$shell_dir" == "fish" ]]; then
                comp_dir="result-${TARGET}/share/fish/vendor_completions.d"
            fi

            if [[ -d "$comp_dir" ]]; then
                for comp in "$comp_dir"/*; do
                    local comp_name=$(basename "$comp")
                    # Only copy completions for this specific binary
                    if [[ "$comp_name" == "${BINARY}"* ]] || [[ "$comp_name" == "_${BINARY}" ]]; then
                        mkdir -p "$STAGE_DIR/share/$(dirname "${comp#result-${TARGET}/share/}")"
                        cp "$comp" "$STAGE_DIR/share/$(dirname "${comp#result-${TARGET}/share/}")/"
                    fi
                done
            fi
        done

        # Only include example config for lex-fetch
        if [[ "$BINARY" == "lex-fetch" ]]; then
            mkdir -p "$STAGE_DIR/share/doc/jacquard-lexicon"
            cp crates/jacquard-lexicon/lexicons.kdl.example "$STAGE_DIR/share/doc/jacquard-lexicon/" 2>/dev/null || true
        fi
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

    # Return to project root
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
