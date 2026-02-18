# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0] - 2026-02-18

### Added

- **Checklist Drag & Drop** reordering
  - Drag items above/below a separator to mark as done/undone
  - Visual feedback while dragging across separator

- **Checklist Separator** between open and completed items
  - Divides checklist into "open" and "done" sections
  - Draggable separator correctly shifts items during drag

- **Sort Modes** for checklists (5 modes)
  - Manual order (default)
  - Alphabetical A→Z / Z→A
  - Creation date (newest/oldest first)

- **Undo** (Ctrl+Z + toolbar button)
  - Full undo history for checklist operations (move, sort, check/uncheck, add, delete)
  - Native CodeMirror undo for text/markdown notes
  - Pointer-based undo stack, max 50 entries

- **Multi-Select** for notes in the sidebar
  - Ctrl+Click and Shift+Click to select multiple notes
  - Long-press (touch/mobile) to activate selection mode
  - Batch delete with confirmation dialog
  - Compact selection-count indicator in sidebar

- **Single-Instance Enforcement**
  - Second launch focuses the existing window instead of opening a duplicate
  - Uses `tauri-plugin-single-instance`

### Changed

- Settings dialog is now fully scrollable with action buttons always visible (no overflow)
- Multi-select bar redesigned: compact count badge instead of full-width blue bar
- Checklist item checkboxes use crisp SVG icons instead of emoji
- Sidebar correctly shows checklist icon and progress bar after WebDAV sync

### Fixed

- Drag-and-drop separator no longer collides with items during drag
- Items now visually toggle state while dragging across the separator
- Settings dialog buttons no longer covered by scrollable content on small windows

### Technical

- Added `UndoStack.js` utility (deep-clone snapshots, pointer-based, max 50 entries)
- `APPIMAGE_EXTRACT_AND_RUN=1` env var in CI for linuxdeploy without FUSE
- `NO_STRIP=true` in CI to skip strip step (linuxdeploy + `.relr.dyn` incompatibility on Ubuntu 22.04)
- Desktop template hardcoded to fix linuxdeploy icon resolution (`{icon}` placeholder was not substituted by AppImage bundler)

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

[Unreleased]: https://github.com/inventory69/simple-notes-desktop/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/inventory69/simple-notes-desktop/releases/tag/v0.1.0
