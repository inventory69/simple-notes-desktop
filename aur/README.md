# simple-notes-desktop-bin

Binary package for Simple Notes Desktop from official GitHub releases.

## Installation

### Via AUR Helper (yay/paru)

```bash
yay -S simple-notes-desktop-bin
# oder
paru -S simple-notes-desktop-bin
```

### Manual Installation

```bash
git clone https://aur.archlinux.org/simple-notes-desktop-bin.git
cd simple-notes-desktop-bin
makepkg -si
```

## AppImage Alternative

If you prefer not to use AUR, download the AppImage:

```bash
# Download from releases
wget https://github.com/inventory69/simple-notes-desktop/releases/download/v0.1.0/simple-notes-desktop_0.1.0_amd64.AppImage

# Make executable
chmod +x simple-notes-desktop_0.1.0_amd64.AppImage

# Run (requires fuse2)
sudo pacman -S fuse2
./simple-notes-desktop_0.1.0_amd64.AppImage

# Optional: Install AppImageLauncher for desktop integration
yay -S appimagelauncher
```

## Building from Source

See [BUILDING.md](https://github.com/inventory69/simple-notes-desktop/blob/main/BUILDING.md)

## Links

- **GitHub**: https://github.com/inventory69/simple-notes-desktop
- **Issues**: https://github.com/inventory69/simple-notes-desktop/issues
