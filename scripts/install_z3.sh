#!/usr/bin/env bash
# install_z3.sh — Downloads a prebuilt z3 binary into tools/z3/bin/
# The binary is not checked into version control (.gitignore excludes it).
# Run this once after cloning: ./scripts/install_z3.sh

set -euo pipefail

Z3_VERSION="4.16.0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALL_DIR="$REPO_ROOT/tools/z3"
BIN_DIR="$INSTALL_DIR/bin"
TMP_DIR="$(mktemp -d)"

cleanup() { rm -rf "$TMP_DIR"; }
trap cleanup EXIT

# Detect OS / arch
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)
        Z3_ASSET="z3-${Z3_VERSION}-x64-glibc-2.39.zip"
        ;;
      aarch64|arm64)
        Z3_ASSET="z3-${Z3_VERSION}-arm64-glibc-2.38.zip"
        ;;
      *)
        echo "Unsupported Linux architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64)
        Z3_ASSET="z3-${Z3_VERSION}-x64-osx-15.7.3.zip"
        ;;
      arm64)
        Z3_ASSET="z3-${Z3_VERSION}-arm64-osx-15.7.3.zip"
        ;;
      *)
        echo "Unsupported macOS architecture: $ARCH" >&2
        exit 1
        ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

Z3_URL="https://github.com/Z3Prover/z3/releases/download/z3-${Z3_VERSION}/${Z3_ASSET}"

# Check if already installed and up to date
if [[ -f "$BIN_DIR/z3" ]]; then
  INSTALLED_VERSION="$("$BIN_DIR/z3" --version 2>/dev/null | grep -oP '\d+\.\d+\.\d+' | head -1 || true)"
  if [[ "$INSTALLED_VERSION" == "$Z3_VERSION" ]]; then
    echo "z3 ${Z3_VERSION} is already installed at $BIN_DIR/z3"
    exit 0
  fi
  echo "Replacing z3 ${INSTALLED_VERSION} with ${Z3_VERSION}..."
fi

echo "Downloading z3 ${Z3_VERSION} for ${OS}/${ARCH}..."
echo "  URL: $Z3_URL"

ZIP_PATH="$TMP_DIR/$Z3_ASSET"
curl -fsSL --progress-bar -o "$ZIP_PATH" "$Z3_URL"

echo "Extracting..."

# Extract using unzip if available, otherwise fall back to python3
extract_zip() {
  local zip="$1" dest="$2"
  if command -v unzip &>/dev/null; then
    unzip -q "$zip" -d "$dest"
  elif command -v python3 &>/dev/null; then
    python3 -c "import zipfile,sys; zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])" "$zip" "$dest"
  else
    echo "Neither unzip nor python3 found. Please install one of them." >&2
    exit 1
  fi
}

extract_zip "$ZIP_PATH" "$TMP_DIR"

# The zip contains a top-level directory like z3-4.16.0-x64-glibc-2.39/
Z3_EXTRACTED_DIR="$(find "$TMP_DIR" -maxdepth 1 -type d -name "z3-*" | head -1)"
if [[ -z "$Z3_EXTRACTED_DIR" ]]; then
  echo "Could not find extracted z3 directory in $TMP_DIR" >&2
  exit 1
fi

mkdir -p "$BIN_DIR"
cp "$Z3_EXTRACTED_DIR/bin/z3" "$BIN_DIR/z3"
chmod +x "$BIN_DIR/z3"

# Verify it works
INSTALLED_VERSION="$("$BIN_DIR/z3" --version | grep -oP '\d+\.\d+\.\d+' | head -1)"
echo "Successfully installed z3 ${INSTALLED_VERSION} → $BIN_DIR/z3"
