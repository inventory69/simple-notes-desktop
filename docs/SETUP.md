# Setup Guide

This guide will help you install and configure Simple Notes Desktop.

## Table of Contents

- [Installation](#installation)
  - [Windows](#windows)
  - [macOS](#macos)
  - [Linux](#linux)
- [WebDAV Server Setup](#webdav-server-setup)
- [Connecting to Your Server](#connecting-to-your-server)
- [Troubleshooting](#troubleshooting)

---

## Installation

### Windows

1. Download the `.msi` or `.exe` installer from the [Releases](https://github.com/inventory69/simple-notes-desktop/releases/latest) page
2. Run the installer
3. Follow the installation wizard
4. Launch "Simple Notes Desktop" from the Start Menu

### macOS

1. Download the `.dmg` file from the [Releases](https://github.com/inventory69/simple-notes-desktop/releases/latest) page
   - **Apple Silicon (M1/M2/M3):** Download `_aarch64.dmg`
   - **Intel:** Download `_x64.dmg`
2. Open the DMG file
3. Drag "Simple Notes Desktop" to your Applications folder
4. Launch from Applications

**If you see "App is damaged" error:**
```bash
xattr -cr "/Applications/Simple Notes Desktop.app"
```

### Linux

#### Debian/Ubuntu (.deb)

```bash
# Download the .deb file, then:
sudo dpkg -i Simple\ Notes\ Desktop_*_amd64.deb
```

#### Fedora/RHEL (.rpm)

```bash
# Download the .rpm file, then:
sudo rpm -i Simple\ Notes\ Desktop-*.x86_64.rpm
```

#### Universal (AppImage)

```bash
# Download the .AppImage file
chmod +x Simple\ Notes\ Desktop_*_amd64.AppImage

# Run it
./Simple\ Notes\ Desktop_*_amd64.AppImage
```

**Required:** Install fuse2 for AppImage support:
```bash
# Arch Linux
sudo pacman -S fuse2

# Debian/Ubuntu
sudo apt install libfuse2

# Fedora
sudo dnf install fuse
```

---

## WebDAV Server Setup

### Option 1: Simple Notes Server (Recommended)

The easiest way to get started is using our Docker-based WebDAV server:

```bash
# Clone the repository
git clone https://github.com/inventory69/simple-notes-sync.git
cd simple-notes-sync/server

# Copy example config
cp .env.example .env

# Edit .env and set your credentials
nano .env  # or your preferred editor

# Start the server
docker compose up -d
```

Your server will be available at: `http://YOUR_IP:8080/`

### Option 2: Nextcloud

If you have a Nextcloud instance, you can use it directly:

**WebDAV URL format:**
```
https://your-nextcloud.com/remote.php/dav/files/USERNAME/Notes/
```

Replace:
- `your-nextcloud.com` with your Nextcloud domain
- `USERNAME` with your Nextcloud username
- `Notes/` with your desired folder (create it first in Nextcloud)

### Option 3: Other WebDAV Servers

Any WebDAV server with Basic Auth support will work:
- Apache with mod_dav
- Nginx with ngx_http_dav_module
- ownCloud
- Synology NAS WebDAV

---

## Connecting to Your Server

1. **Launch** Simple Notes Desktop
2. **Enter your WebDAV details:**
   - **Server URL:** Your WebDAV endpoint (e.g., `http://192.168.1.100:8080/`)
   - **Username:** Your WebDAV username
   - **Password:** Your WebDAV password
3. **Click "Connect"**
4. Your notes will sync automatically

### Tips

- Use **http://** for local servers
- Use **https://** for remote servers (recommended)
- The URL should point to the notes folder, not the root

---

## Troubleshooting

### Connection Failed

1. **Check the URL format** - Make sure it ends with `/`
2. **Verify credentials** - Test them with a WebDAV client like Cyberduck
3. **Check server logs** - Look for authentication errors
4. **Firewall** - Ensure the port is open

### Linux: AppImage doesn't start

AppImage requires fuse2:
```bash
# Arch Linux
sudo pacman -S fuse2

# Debian/Ubuntu
sudo apt install libfuse2
```

### macOS: "App is damaged"

This happens because the app isn't notarized by Apple:
```bash
xattr -cr "Simple Notes Desktop.app"
```

### Sync Conflicts

If the same note is edited on multiple devices simultaneously:
- The most recent change wins
- Previous versions are not currently preserved

**Best practice:** Let one device finish syncing before editing on another.

### SSL Certificate Errors

For self-signed certificates on your WebDAV server:
1. Add the certificate to your system's trust store
2. Or use HTTP for local network servers (acceptable for private networks)

---

## Next Steps

- 📱 Install [Simple Notes Sync](https://github.com/inventory69/simple-notes-sync) on your Android device
- 🌐 Try [Simple Notes Web](https://github.com/inventory69/simple-notes-web) for browser access
- 📖 Read the [Architecture docs](ARCHITECTURE.md) if you want to contribute
