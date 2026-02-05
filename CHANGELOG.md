# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-02-05

### Added

- **System Tray** support
  - Minimize to tray on window close (optional)
  - Tray icon with context menu (Show Window / Quit)
  - Left-click tray icon to restore window

- **Autostart** on system boot (optional)
  - Uses native platform autostart mechanism
  - Toggle in Settings dialog

- **New App Icon** — custom-designed icon replacing the default Tauri icon
  - All platform sizes generated (32x32 through 512x512)
  - Windows ICO, macOS ICNS
  - Proper Wayland/KDE Plasma taskbar icon via desktop entry

- **Custom Dialog System** (`DialogService`)
  - Styled confirm, alert, prompt, and error dialogs
  - Replaces native browser dialogs for consistent look

### Changed

- Default window height increased from 700 to 850px
- Improved button styling in dialogs for modern look
- Settings dialog now includes tray and autostart toggles

### Fixed

- AppImage build on NTFS filesystems (local dev on Arch Linux)
- Window icon not showing on Wayland/KDE Plasma taskbar
- Strip configuration causing build failures

### Technical

- Added `tray-icon` and `image-png` features to Tauri
- Added `tauri-plugin-autostart` dependency
- Programmatic window icon setting for Wayland compatibility
- Desktop entry file for proper icon resolution on Wayland compositors
- Build script fallback for non-ext4 filesystems

## [0.1.0] - 2026-02-05

### Added

- **Markdown Editor** with CodeMirror 6
  - Syntax highlighting
  - Line numbers
  - Line wrapping
  - Keyboard shortcuts

- **Live Preview** toggle for rendered Markdown

- **Checklist Support**
  - Create/edit checklists
  - Tap-to-check items
  - Add/remove items

- **WebDAV Synchronization**
  - Connect to any WebDAV server
  - Nextcloud compatible
  - Local server support (localhost, private IPs)
  - Secure credential storage

- **Theme Support**
  - Light mode
  - Dark mode
  - System preference detection
  - KDE Breeze theme integration

- **Auto-save** with debouncing

- **Note Search** by title and content

- **Cross-platform builds**
  - Windows: MSI, EXE
  - macOS: DMG (Intel + Apple Silicon)
  - Linux: DEB, RPM, AppImage

- **About Dialog** with version info and GitHub link

### Technical

- Built with Tauri 2.0 and Rust backend
- WebKit environment variable fixes for Linux/Wayland
- Automatic AppImage creation fallback

---

[Unreleased]: https://github.com/inventory69/simple-notes-desktop/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/inventory69/simple-notes-desktop/releases/tag/v0.1.0
