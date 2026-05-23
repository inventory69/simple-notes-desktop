# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.0] - 2026-05-23

### Fixed

- **Cross-app data loss: Android fields now preserved on desktop save**
  - Desktop round-trips (`save_note`) were silently dropping all fields unknown to the Rust model,
    including `color`, `labels`, `isPinned`, `importedAt` (Note) and `originalOrder`, `createdAt`,
    `indentationLevel` (ChecklistItem) — causing Android note colors and labels to disappear after
    any desktop edit
  - `Note` and `ChecklistItem` now carry explicit fields for the near-term Android v2.5.0 features
    plus a `#[serde(flatten)]` catch-all for future fields; no desktop-visible changes
  - Markdown export updated accordingly: `color`, `labels`, `pinned`, `imported` frontmatter lines
    written when set (matching Android `Note.toMarkdown` format)

- **Editor reloads open note after server sync**
  - "Sync Now" previously refreshed the sidebar but left the editor showing stale content;
    a subsequent save would silently overwrite the newer server version
  - If the server version is newer and the editor is clean, the note reloads silently;
    if there are unsaved changes a conflict dialog is shown ("Load Server Version" / "Keep Mine")

- **Ghost saves eliminated when switching notes after autosave** (`fix(editor)`)
  - `scheduleSave()` left a stale timer ID set, causing every note switch after an autosave to
    flush an extra save of the previous note; Android reported 2–3 notes synced when only one
    was edited
  - Save now snapshots `currentNote` before the network `await` so a note switch mid-request
    can no longer overwrite the wrong note

- **Stale markdown file deleted when note title changes** (`fix(sync)`)
  - Renaming a note now fetches the current server title before writing; if the title changed the
    old `{title}.md` is deleted first, preventing `.md` file accumulation in the `-md/` folder

- **Markdown `sort:` field written as SCREAMING_SNAKE_CASE** (`fix(sync)`)
  - Desktop was writing `sort: unchecked_first`; Android writes `UNCHECKED_FIRST` (raw enum value)

- **Preview toggle button state reset when switching notes** (`fix(ui)`)
  - `previewToggleBtn` kept its active (blue) highlight when switching away from a previewed note

- **Linux/AppImage crash on Fedora Silverblue 41+ and similar immutable distros** (`fix(linux)`)
  - WebKit sandbox conflicts with the host container runtime, causing an EGL_BAD_PARAMETER crash
    at startup; GStreamer VA-API probe before WebKit init caused a secondary crash
  - Fixed by setting `WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1` and `GST_VAAPI_ALL_DRIVERS=1`
    in `main.rs` (before Tauri init) and in the deb/rpm launcher script
  - Thanks to [@daalja](https://github.com/daalja) for the detailed report and workaround — closes [#1](https://github.com/inventory69/simple-notes-desktop/issues/1)

- **COLRv1 emoji crash in AppImage on Fedora Silverblue / older FreeType** (`fix(ui)`)
  - Certain notes containing emoji triggered a bounds-check abort in colrv1_configure_skpaint
    when the host's Noto Color Emoji (COLRv1 format) was rendered via an older bundled FreeType
  - Fixed by setting `font-variant-emoji: text` globally; emoji render in monochrome instead of color
  - Thanks to [@daalja](https://github.com/daalja) for identifying the affected note and reproducing the crash — closes [#5](https://github.com/inventory69/simple-notes-desktop/issues/5)

### Changed

- **`Settings` deserialization simplified** (`refactor(storage)`)
  - Added `#[serde(default)]` to `Settings`; `get_settings` now deserializes from a single JSON
    map instead of reading each key individually — adding a new setting only requires updating the
    struct and `impl Default`

### Removed

- **Dead markdown parse path removed** (`refactor(sync)`)
  - `parse_markdown`, `iso_to_timestamp`, and all associated regex helpers and unit tests deleted
    from `markdown.rs`; the desktop only ever reads JSON, markdown is write-only export

### Documentation

- Stale v0.2.0 version footer and dead `docs/ARCHITECTURE.md` link in README fixed
- Contributing guide updated for current tooling, Biome semicolons convention, and full
  Conventional Commit type list

## [0.4.0] - 2026-03-03

### Added

- **Configurable Sync Folder** (Android feature parity)
  - WebDAV sync folder is now configurable instead of hardcoded `/notes/`
  - New "Advanced" section in Settings dialog with sync folder input
  - New "Advanced" toggle in Connection dialog for setting folder before first connect
  - Input sanitization: only alphanumeric, dash and underscore allowed (max 50 chars)
  - Changing the sync folder in Settings triggers automatic reconnect with notes reload

- **3-line Sidebar Previews**
  - Text notes show up to 3 lines of content instead of a single truncated line
  - Checklist notes show first 3 items with ☐/☑ icons instead of count-only summary
  - Checklists with more than 3 items show additional "X/X completed" summary line

- **Real-time Sidebar Updates** while editing (title, content, checklist changes reflect immediately)

### Changed

- Autosave debounce increased from 1s to 3s (matches Android app behavior)

### Fixed

- GTK Client-Side Decorations removed on Linux — KWin now uses native Server-Side Decorations
- Frozen titlebar buttons after tray hide/show on KDE/Wayland (use `gtk_window.present()`)
- Backspace in empty checklist item no longer deletes the item (only blur + ✕ button do)
- Checklist separator position when all items are checked or only one item exists
- Trailing "0 completed" separator on fully unchecked checklists removed
- Checklist item order gaps after deletions (sequential renormalization)
- Manual sort mode now groups unchecked above checked while preserving drag order
- Editor now clears when switching sync folders (no stale note from previous folder)
- First note no longer appears visually selected after connecting without opening it

## [0.3.1] - 2026-02-20

### Added

- Checklist items now support multiline text with word wrap and vertical scrolling
- Scroll-based gradient indicators on checklist items (matching Android parity)

### Changed

- Checklist `<input type="text">` replaced with `<textarea>` for multiline support
- Debug `println!` replaced with conditional `eprintln!` (debug builds only, stderr)
- `fix_note_type()` uses idiomatic `if let` instead of `is_some()` + `unwrap()`
- UUID regex in WebDAV client compiled once via `LazyLock` instead of per-call
- `std::env::set_var` wrapped in `unsafe` block with safety documentation
- AppImage filename in `build.mjs` reads version dynamically from `package.json`
- Workflow release notes generated automatically from CHANGELOG.md

### Fixed

- Save race condition when quickly switching between notes (pending save now flushes)
- Removed unused `syncIcon` variable in `main.js`
- Select-multiple button now uses a distinct list-with-checkboxes icon (no longer identical to the New Checklist button)
- Select-multiple button highlights blue while selection mode is active
- Batch delete button is now a clearly visible red "Delete" button (was: invisible icon-only)
- Added toolbar separator between create-actions and multi-select button

### Technical

- Added `scripts/bump-version.sh` for single-command version updates across 5 files
- Added `scripts/update-aur-sha.sh` for post-release AUR sha256sum updates
- Added `pnpm bump` and `pnpm update-aur-sha` convenience scripts
- Linting: Biome (JS/CSS) + Clippy/rustfmt (Rust) with CI enforcement

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

[Unreleased]: https://github.com/inventory69/simple-notes-desktop/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/inventory69/simple-notes-desktop/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/inventory69/simple-notes-desktop/releases/tag/v0.1.0
