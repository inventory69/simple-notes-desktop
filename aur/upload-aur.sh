#!/bin/bash
# AUR Package Upload Script for simple-notes-desktop-bin
# Usage: ./upload-aur.sh
#
# Prerequisites:
#   1. SSH key registered at https://aur.archlinux.org/account/inventory69
#   2. git configured with your AUR account
#
# First-time setup:
#   ssh-keygen -t ed25519 -C "aur@inventory69"
#   # Add ~/.ssh/id_ed25519.pub to AUR account settings
#   # Add to ~/.ssh/config:
#   #   Host aur.archlinux.org
#   #     IdentityFile ~/.ssh/id_ed25519
#   #     User aur

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKGNAME="simple-notes-desktop-bin"
TMPDIR=$(mktemp -d)

echo "=== AUR Upload for ${PKGNAME} ==="
echo ""

# Check SSH access
echo "[1/5] Testing AUR SSH access..."
SSH_OUTPUT=$(ssh -T aur@aur.archlinux.org 2>&1 || true)
if echo "$SSH_OUTPUT" | grep -q "Welcome to AUR"; then
    echo "  ✓ SSH access OK"
else
    echo "ERROR: SSH authentication to AUR failed!"
    echo "Output: $SSH_OUTPUT"
    echo ""
    echo "Setup steps:"
    echo "  1. Generate SSH key: ssh-keygen -t ed25519 -C 'aur'"
    echo "  2. Add public key to: https://aur.archlinux.org/account/inventory69 -> SSH Public Key"
    echo "  3. Add to ~/.ssh/config:"
    echo "     Host aur.archlinux.org"
    echo "       IdentityFile ~/.ssh/id_ed25519_aur"
    echo "       User aur"
    exit 1
fi

# Clone or create AUR repo
echo "[2/5] Cloning AUR repository..."
cd "${TMPDIR}"
if git clone ssh://aur@aur.archlinux.org/${PKGNAME}.git 2>/dev/null; then
    echo "  ✓ Existing repo cloned"
else
    echo "  Creating new AUR package..."
    git clone ssh://aur@aur.archlinux.org/${PKGNAME}.git 2>/dev/null || {
        mkdir "${PKGNAME}"
        cd "${PKGNAME}"
        git init
        git remote add origin ssh://aur@aur.archlinux.org/${PKGNAME}.git
    }
    echo "  ✓ New repo initialized"
fi

cd "${PKGNAME}"

# Copy PKGBUILD and .SRCINFO
echo "[3/5] Copying package files..."
cp "${SCRIPT_DIR}/PKGBUILD" .
cp "${SCRIPT_DIR}/.SRCINFO" .
echo "  ✓ Files copied"

# Validate PKGBUILD
echo "[4/5] Validating PKGBUILD..."
if command -v namcap &>/dev/null; then
    namcap PKGBUILD || true
fi
echo "  ✓ Validation complete"

# Commit and push
echo "[5/5] Pushing to AUR..."
git add PKGBUILD .SRCINFO
git commit -m "Update ${PKGNAME} to $(grep pkgver= PKGBUILD | cut -d= -f2)" 2>/dev/null || {
    echo "  No changes to commit"
    rm -rf "${TMPDIR}"
    exit 0
}
git push origin master

echo ""
echo "=== SUCCESS ==="
echo "Package available at: https://aur.archlinux.org/packages/${PKGNAME}"
echo ""

# Cleanup
rm -rf "${TMPDIR}"
