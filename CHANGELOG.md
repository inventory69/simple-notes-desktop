# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/inventory69/simple-notes-desktop/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/inventory69/simple-notes-desktop/releases/tag/v0.1.0
