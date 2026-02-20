#!/usr/bin/env bash
# Build wasm-bodge for a given target and package the result.
#
# Usage (inside the container, or locally if toolchains are present):
#   ./docker-build/build.sh <target> <version-tag>
#
# Example:
#   ./docker-build/build.sh x86_64-unknown-linux-musl v0.1.0
#
# Outputs a ready-to-upload archive in the project root:
#   wasm-bodge-<tag>-<target>.tar.gz   (Unix targets)
#   wasm-bodge-<tag>-<target>.zip      (Windows targets)

set -euo pipefail

BINARY_NAME="wasm-bodge"
SUPPORTED_TARGETS=(
    x86_64-unknown-linux-musl
    aarch64-unknown-linux-musl
    x86_64-apple-darwin
    aarch64-apple-darwin
    x86_64-pc-windows-gnu
)

# ── Argument validation ────────────────────────────────────────────────────────

usage() {
    echo "Usage: $0 <target> <version-tag>"
    echo ""
    echo "Supported targets:"
    for t in "${SUPPORTED_TARGETS[@]}"; do echo "  $t"; done
    exit 1
}

TARGET="${1:-}"
TAG="${2:-}"

if [[ -z "$TARGET" || -z "$TAG" ]]; then
    echo "Error: target and version tag are required."
    usage
fi

VALID=0
for t in "${SUPPORTED_TARGETS[@]}"; do
    [[ "$t" == "$TARGET" ]] && VALID=1 && break
done
if [[ "$VALID" -eq 0 ]]; then
    echo "Error: unsupported target '$TARGET'"
    usage
fi

# ── macOS linker detection ─────────────────────────────────────────────────────
# osxcross binary names include the Darwin version (e.g. x86_64-apple-darwin20.4-clang).
# We detect the suffix at runtime rather than hardcoding it.

configure_macos_linkers() {
    local OSXCROSS_BIN="/opt/osxcross/target/bin"
    if [[ ! -d "$OSXCROSS_BIN" ]]; then
        echo "Error: osxcross not found at $OSXCROSS_BIN. Are you running inside the build container?" >&2
        exit 1
    fi

    X86_CLANG=$(find "$OSXCROSS_BIN" -name "x86_64-apple-darwin*-clang" | head -n1)
    ARM_CLANG=$(find "$OSXCROSS_BIN" -name "arm64-apple-darwin*-clang" -o -name "aarch64-apple-darwin*-clang" 2>/dev/null | head -n1)

    if [[ -z "$X86_CLANG" || -z "$ARM_CLANG" ]]; then
        echo "Error: could not find osxcross clang wrappers in $OSXCROSS_BIN" >&2
        exit 1
    fi

    echo "Detected osxcross toolchain:"
    echo "  x86_64: $X86_CLANG"
    echo "  arm64:  $ARM_CLANG"

    # Expose as env vars for cargo; these override any static config.toml values.
    export CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER="$X86_CLANG"
    export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER="$ARM_CLANG"
    export CC_x86_64_apple_darwin="$X86_CLANG"
    export CC_aarch64_apple_darwin="$ARM_CLANG"
}

if [[ "$TARGET" == *apple-darwin* ]]; then
    configure_macos_linkers
fi

# ── Build ──────────────────────────────────────────────────────────────────────

echo ""
echo "=== Building $BINARY_NAME for $TARGET ==="
echo ""

cargo build --release --target "$TARGET"

# ── Package ────────────────────────────────────────────────────────────────────

echo ""
echo "=== Packaging ==="

BINARY_PATH="target/$TARGET/release/$BINARY_NAME"

if [[ "$TARGET" == *windows* ]]; then
    BINARY_PATH="${BINARY_PATH}.exe"
    ARCHIVE="${BINARY_NAME}-${TAG}-${TARGET}.zip"
    cp "$BINARY_PATH" "${BINARY_NAME}.exe"
    zip "$ARCHIVE" "${BINARY_NAME}.exe"
    rm "${BINARY_NAME}.exe"
else
    ARCHIVE="${BINARY_NAME}-${TAG}-${TARGET}.tar.gz"
    cp "$BINARY_PATH" "$BINARY_NAME"
    tar czf "$ARCHIVE" "$BINARY_NAME"
    rm "$BINARY_NAME"
fi

echo "Created: $ARCHIVE"
