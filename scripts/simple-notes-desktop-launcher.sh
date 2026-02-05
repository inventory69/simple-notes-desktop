#!/bin/bash
# Simple Notes Desktop Launcher
# This script sets the necessary environment variables for Wayland compatibility

# WebKit environment variables for Wayland
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Run the application
exec "$SCRIPT_DIR/simple-notes-desktop" "$@"
