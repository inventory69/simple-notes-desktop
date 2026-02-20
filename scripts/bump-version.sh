#!/bin/bash
# bump-version.sh - Update version across all project files
# Usage: ./scripts/bump-version.sh <new-version>
# Example: ./scripts/bump-version.sh 0.3.1
#
# Updates version in:
#   1. package.json
#   2. src-tauri/Cargo.toml
#   3. src-tauri/tauri.conf.json
#   4. aur/PKGBUILD (pkgver only, sha256sum must be updated after release)
#   5. aur/.SRCINFO (pkgver and source URL)

set -euo pipefail

# ─── Argument validation ───────────────────────────────────────────────
if [ $# -ne 1 ]; then
  echo "Usage: $0 <new-version>"
  echo "Example: $0 0.3.1"
  exit 1
fi

NEW_VERSION="$1"

# Validate semver format (major.minor.patch, optional pre-release)
if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.]+)?$'; then
  echo "ERROR: Invalid version format: $NEW_VERSION"
  echo "Expected: MAJOR.MINOR.PATCH (e.g. 0.3.1, 1.0.0-beta.1)"
  exit 1
fi

# ─── Determine project root ───────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Read current version from package.json (source of truth)
CURRENT_VERSION=$(node -e "console.log(require('./package.json').version)")
echo "Current version: $CURRENT_VERSION"
echo "New version:     $NEW_VERSION"
echo ""

if [ "$CURRENT_VERSION" = "$NEW_VERSION" ]; then
  echo "WARNING: Version is already $NEW_VERSION"
  exit 0
fi

# ─── 1. package.json ──────────────────────────────────────────────────
echo "[1/5] Updating package.json..."
node -e "
const fs = require('fs');
const pkg = JSON.parse(fs.readFileSync('package.json', 'utf-8'));
pkg.version = '$NEW_VERSION';
fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
"
echo "  ✓ package.json → $NEW_VERSION"

# ─── 2. src-tauri/Cargo.toml ──────────────────────────────────────────
echo "[2/5] Updating src-tauri/Cargo.toml..."
sed -i "s/^version = \"$CURRENT_VERSION\"/version = \"$NEW_VERSION\"/" src-tauri/Cargo.toml
echo "  ✓ Cargo.toml → $NEW_VERSION"

# ─── 3. src-tauri/tauri.conf.json ─────────────────────────────────────
echo "[3/5] Updating src-tauri/tauri.conf.json..."
node -e "
const fs = require('fs');
const conf = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf-8'));
conf.version = '$NEW_VERSION';
fs.writeFileSync('src-tauri/tauri.conf.json', JSON.stringify(conf, null, 2) + '\n');
"
echo "  ✓ tauri.conf.json → $NEW_VERSION"

# ─── 4. aur/PKGBUILD ──────────────────────────────────────────────────
echo "[4/5] Updating aur/PKGBUILD..."
sed -i "s/^pkgver=.*/pkgver=$NEW_VERSION/" aur/PKGBUILD
sed -i "s/^pkgrel=.*/pkgrel=1/" aur/PKGBUILD
sed -i "s/^sha256sums=.*/sha256sums=('SKIP')/" aur/PKGBUILD
echo "  ✓ PKGBUILD → $NEW_VERSION (sha256sums='SKIP' – update after release!)"

# ─── 5. aur/.SRCINFO ──────────────────────────────────────────────────
echo "[5/5] Updating aur/.SRCINFO..."
sed -i "s/pkgver = $CURRENT_VERSION/pkgver = $NEW_VERSION/" aur/.SRCINFO
sed -i "s/pkgrel = .*/pkgrel = 1/" aur/.SRCINFO
sed -i "s|simple-notes-desktop-bin-${CURRENT_VERSION}.deb::https://github.com/inventory69/simple-notes-desktop/releases/download/v${CURRENT_VERSION}/Simple.Notes.Desktop_${CURRENT_VERSION}_amd64.deb|simple-notes-desktop-bin-${NEW_VERSION}.deb::https://github.com/inventory69/simple-notes-desktop/releases/download/v${NEW_VERSION}/Simple.Notes.Desktop_${NEW_VERSION}_amd64.deb|" aur/.SRCINFO
sed -i "s/sha256sums = .*/sha256sums = SKIP/" aur/.SRCINFO
echo "  ✓ .SRCINFO → $NEW_VERSION"

# ─── Summary ──────────────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════"
echo "  Version bumped: $CURRENT_VERSION → $NEW_VERSION"
echo "══════════════════════════════════════════════════"
echo ""
echo "Updated files:"
echo "  • package.json"
echo "  • src-tauri/Cargo.toml"
echo "  • src-tauri/tauri.conf.json"
echo "  • aur/PKGBUILD"
echo "  • aur/.SRCINFO"
echo ""
echo "⚠ IMPORTANT: After release, update sha256sum in PKGBUILD:"
echo "    sha256sum Simple.Notes.Desktop_${NEW_VERSION}_amd64.deb"
echo "    # Then run: ./scripts/update-aur-sha.sh $NEW_VERSION"
echo ""
echo "Next steps:"
echo "  1. Update CHANGELOG.md with v$NEW_VERSION notes"
echo "  2. git add -A && git commit -m \"chore: bump version to $NEW_VERSION\""
echo "  3. git tag v$NEW_VERSION && git push origin main --tags"
echo "  4. After release: ./scripts/update-aur-sha.sh $NEW_VERSION && ./aur/upload-aur.sh"
