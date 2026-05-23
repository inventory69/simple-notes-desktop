#!/bin/bash
# Simple Notes Desktop Launcher
# This script sets the necessary environment variables for Wayland compatibility

# WebKit environment variables for Wayland
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_COMPOSITING_MODE=1
# Disable WebKit sandbox — required on some immutable distros where the sandbox
# conflicts with the container runtime and causes EGL_BAD_PARAMETER at startup
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
# Prevent GStreamer VA-API probe from touching EGL before WebKit is ready
export GST_VAAPI_ALL_DRIVERS=1

# Get the directory of this script
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Run the application
exec "$SCRIPT_DIR/simple-notes-desktop" "$@"
