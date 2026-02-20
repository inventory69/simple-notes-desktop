#!/bin/bash
# update-aur-sha.sh - Download released .deb and update sha256sum in PKGBUILD + .SRCINFO
# Usage: ./scripts/update-aur-sha.sh [version]
# If no version given, reads from package.json
#
# Run this AFTER the GitHub Release has been published (not draft).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Determine version
if [ $# -ge 1 ]; then
  VERSION="$1"
else
  VERSION=$(node -e "console.log(require('./package.json').version)")
fi

REPO_URL="https://github.com/inventory69/simple-notes-desktop"
DEB_FILENAME="Simple.Notes.Desktop_${VERSION}_amd64.deb"
DEB_URL="${REPO_URL}/releases/download/v${VERSION}/${DEB_FILENAME}"
TMPFILE=$(mktemp)

echo "=== Update AUR sha256sum for v${VERSION} ==="
echo ""

# Download .deb
echo "[1/3] Downloading ${DEB_FILENAME}..."
if ! curl -fSL -o "$TMPFILE" "$DEB_URL"; then
  echo "ERROR: Download failed. Is the release published (not draft)?"
  echo "URL: $DEB_URL"
  rm -f "$TMPFILE"
  exit 1
fi
echo "  ✓ Downloaded $(du -h "$TMPFILE" | cut -f1)"

# Calculate sha256sum
echo "[2/3] Calculating sha256sum..."
SHA256=$(sha256sum "$TMPFILE" | cut -d' ' -f1)
echo "  ✓ $SHA256"
rm -f "$TMPFILE"

# Update PKGBUILD and .SRCINFO
echo "[3/3] Updating aur/PKGBUILD and aur/.SRCINFO..."
sed -i "s/^sha256sums=.*/sha256sums=('${SHA256}')/" aur/PKGBUILD
sed -i "s/sha256sums = .*/sha256sums = ${SHA256}/" aur/.SRCINFO
echo "  ✓ Updated"

echo ""
echo "Done! Run: ./aur/upload-aur.sh"
