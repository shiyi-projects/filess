#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────
# bundle_python_macos.sh
# Downloads a standalone Python build for macOS and prepares
# python-runtime/ for Tauri resource bundling.
#
# Usage:
#   ./scripts/bundle_python_macos.sh              # auto-detect arch
#   ./scripts/bundle_python_macos.sh aarch64      # Apple Silicon
#   ./scripts/bundle_python_macos.sh x86_64       # Intel
# ──────────────────────────────────────────────────────────
set -euo pipefail

VERSION="3.11.15"
ARCH="${1:-$(uname -m)}"          # aarch64 or x86_64
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TAURI_DIR="$ROOT/apps/desktop/src-tauri"
RUNTIME_DIR="$TAURI_DIR/python-runtime"
SIDECAR_SRC="$ROOT/services/sidecar/src/sidecar"

# python-build-standalone release tag & URL
# Repo migrated from indygreg → astral-sh
PBS_TAG="20260414"
PBS_BASE="https://github.com/astral-sh/python-build-standalone/releases/download/$PBS_TAG"
TARBALL="cpython-${VERSION}+${PBS_TAG}-${ARCH}-apple-darwin-install_only.tar.gz"
URL="$PBS_BASE/$TARBALL"
TEMP_TAR="$TAURI_DIR/$TARBALL"

echo ""
echo "=== Bundle Python Standalone (macOS) ==="
echo "  Version : $VERSION ($ARCH)"
echo "  URL     : $URL"
echo "  Target  : $RUNTIME_DIR"
echo ""

# 1. Clean previous
if [ -d "$RUNTIME_DIR" ]; then
    echo "-> Removing old python-runtime/ ..."
    rm -rf "$RUNTIME_DIR"
fi

# 2. Download (with cache)
if [ ! -f "$TEMP_TAR" ]; then
    echo "-> Downloading $TARBALL ..."
    curl -fSL -o "$TEMP_TAR" "$URL"
else
    echo "-> Using cached $TARBALL"
fi

# 3. Extract — the tarball contains a `python/` top-level dir
echo "-> Extracting..."
mkdir -p "$RUNTIME_DIR"
tar -xzf "$TEMP_TAR" -C "$RUNTIME_DIR" --strip-components=1

# 4. Trim to reduce size (~30MB → ~15MB)
echo "-> Trimming unnecessary files..."
rm -rf "$RUNTIME_DIR/include"
rm -rf "$RUNTIME_DIR/share"
rm -rf "$RUNTIME_DIR/lib/python3.11/test"
rm -rf "$RUNTIME_DIR/lib/python3.11/unittest"
rm -rf "$RUNTIME_DIR/lib/python3.11/idlelib"
rm -rf "$RUNTIME_DIR/lib/python3.11/tkinter"
rm -rf "$RUNTIME_DIR/lib/python3.11/turtle*"
rm -rf "$RUNTIME_DIR/lib/python3.11/ensurepip"
rm -rf "$RUNTIME_DIR/lib/python3.11/lib2to3"
find "$RUNTIME_DIR" -name "*.pyc" -delete
find "$RUNTIME_DIR" -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null || true

# 5. Copy sidecar source
echo "-> Copying sidecar source code..."
SIDECAR_DEST="$RUNTIME_DIR/sidecar"
cp -R "$SIDECAR_SRC" "$SIDECAR_DEST"
find "$SIDECAR_DEST" -name "__pycache__" -type d -exec rm -rf {} + 2>/dev/null || true

# 6. Summary
TOTAL_SIZE=$(du -sh "$RUNTIME_DIR" | cut -f1)
FILE_COUNT=$(find "$RUNTIME_DIR" -type f | wc -l | tr -d ' ')
echo ""
echo "OK  python-runtime/ ready"
echo "    Files : $FILE_COUNT"
echo "    Size  : $TOTAL_SIZE"
echo ""
