# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.1] - 2026-06-30

### Fixed

- `.hidden` elements are now reliably hidden even when overridden by higher-specificity component rules ([ba15bff](https://github.com/inventory69/simple-notes-desktop/commit/ba15bff7da547f26eaad807b233b65f938ee72ae))

## [0.9.0] - 2026-06-30

### Added

- Local-first offline architecture: all note operations write to local storage first and sync to WebDAV in the background, making the app fully functional without a server connection ([44fbb2c](https://github.com/inventory69/simple-notes-desktop/commit/44fbb2c989ccab7656561630c06329455b1df2c5))
  - A scheduler (5s debounce + 5-minute periodic) reconciles local changes with the server and emits `notes-synced` so the UI reloads without a full refresh
  - Connection dialog replaced with a Settings screen: offline toggle, server fields, non-destructive "Test connection" button
  - Sync status shown as an inline badge on the sync button instead of a modal dialog
  - Per-folder `local_only` flag; local-only folders never sync to the server
  - One-time migration moves existing `note_cache` data into the new `local_store`
  - New IPC commands: `disconnect`, `is_connected`, `test_connection`
- Settings reorganised into labelled card groups (Server, Sync, Editor, Appearance, App) with the offline-mode toggle and Test Connection button at the top ([44fbb2c](https://github.com/inventory69/simple-notes-desktop/commit/44fbb2c989ccab7656561630c06329455b1df2c5))
- 15-theme system with visual swatch picker: Breeze Light/Dark, Catppuccin Latte/Frappé/Macchiato/Mocha, Nord, Gruvbox Dark/Light, Tokyo Night, Rosé Pine, Rosé Pine Dawn, plus System/Light/Dark ([71b2de9](https://github.com/inventory69/simple-notes-desktop/commit/71b2de9b35a2b5c7686c08190a2e9a19692310a8))
  - Token-based architecture with 8 new CSS custom properties; `data-mode="light|dark"` decouples dark-mode rules from specific theme IDs
  - Theme selector replaced with a visual swatch grid (3-dot color preview per theme)

### Fixed

- Selection background now uses theme colors in all 15 themes; note-list preview links are styled with `--color-primary` and open in the system browser ([de9e6fe](https://github.com/inventory69/simple-notes-desktop/commit/de9e6fe02207ba22dfc6a284be2619112414c046))
  - CodeMirror's baseTheme specificity required a `!important` override on `.cm-selectionBackground` resolved via `color-mix`
  - Preview links previously had `pointer-events:none` and fell back to browser-default blue
- WebDAV connection now fails fast with a 5-second timeout for unreachable hosts instead of hanging for 30 seconds ([eea5150](https://github.com/inventory69/simple-notes-desktop/commit/eea5150b07b1ddece59878e5c35e721494b3166a))
- CodeMirror gutter background now correctly uses theme tokens instead of the hardcoded `#f5f5f5` from `basicSetup` ([4e49888](https://github.com/inventory69/simple-notes-desktop/commit/4e49888d80c26ef3ef0073e3ea13e7d23c52be83))

### Changed

- Live Markdown preview and note-list search are debounced (200ms and 150ms respectively) to avoid redundant rendering while typing ([10b4f21](https://github.com/inventory69/simple-notes-desktop/commit/10b4f218594b2a8d18f690c49d0e95f351570108))

## [0.8.0] - 2026-06-16

### Added

- Background sync engine with conflict detection and resolution (`feat(sync)`)
  - PENDING notes are pushed to the server on each sync run; a disk-persisted note cache tracks each note's last-synced timestamp and status between runs
  - Server notes newer than the last sync are flagged CONFLICT; server-side deletions become DELETED_ON_SERVER
  - ⚡ conflict and 🗑 deleted-on-server badges in the note list open a keep-mine / use-server resolve dialog
  - `sync` and `resolve_conflict` IPC commands; a sync lock prevents concurrent runs
- Trash with soft-delete, restore, and permanent delete, matching Android (`feat(trash)`)
  - Deleting a note now moves it to trash (`trashedAt`) instead of removing it; a dedicated trash view shows trashed notes with a countdown to permanent deletion, plus Restore, Delete permanently, and Empty Trash actions
  - Permanent deletes are recorded in a shared `deletions.json` ledger so removals propagate across devices; `list_notes` zombie-cleans notes the ledger says were deleted elsewhere
- Local-only folders with offline note storage (`feat(sync)`)
  - Notes in local-only folders are stored in an on-disk JSON sidecar instead of the WebDAV server; all CRUD routes through the local store
  - Folder context menu toggles "Make local-only / Include in sync" with a Remove / Keep / Cancel choice for existing server copies
  - An offline sync queue persists folder tombstones and pending deletions, drained on the next successful connect
- Markdown syntax highlighting and a formatting toolbar in the editor (`feat(editor)`)
  - CodeMirror highlights headings, bold, italic, strikethrough, code spans, and links, dimming syntax markers to match Android
  - 9-button toolbar (bold, italic, strikethrough, heading, code, link, bullet list, checklist item, horizontal rule) with Ctrl+B / Ctrl+I shortcuts
- Inline markdown rendering in note-list previews — bold, italic, strikethrough, inline code, and links render in sidebar preview lines for text notes (`feat(ui)`)
- Font size setting with a five-level chip selector (small/system/normal/large/xlarge), applied via a `--font-scale` custom property with live preview (`feat(ui)`)
- Default open mode for text notes — open existing text notes directly in preview when configured (`feat(settings)`)
- Color accent line on the editor header for notes with a color, matching the Android toolbar accent (`feat(editor)`)
- New text notes auto-focus the content area; new checklists create and focus an initial empty item (`feat(editor)`)
- Auto-scroll the checklist container while dragging an item to the top or bottom edge during reorder (`feat(checklist)`)

### Fixed

- Deleting a folder without keeping its notes now moves them to trash instead of hard-deleting them (`fix(sync)`)
- Folder rename and delete now clean up the old WebDAV JSON and Markdown directories instead of leaving them orphaned (`fix(sync)`)
- Excluding a folder from sync with "Remove from server" now deletes the folder directories on the server, matching Android (`fix(sync)`)
- Re-including a local-only folder now removes its stale tombstones from `deletions.json`, preventing re-uploaded notes from being treated as deleted (`fix(sync)`)
- Checklist items now move to the correct sort group immediately on checkbox toggle in MANUAL sort mode, with focus preserved (`fix(checklist)`)

## [0.7.0] - 2026-06-01

### Added

- Folder support: notes can be organised into folders, matching Android app v2.7.0 (`feat(folders)`)
  - **Requires Android app ≥ 2.7.0** — notes moved into folders are stored in subdirectories on the WebDAV server and will not be visible to older Android versions
  - Folders appear in the sidebar as cards with note-count badges; right-click for rename, delete, or color
  - Deleting a non-empty folder offers a "Keep notes (move to Root)" checkbox
  - Multi-select toolbar gains a "Move" button to batch-move selected notes into any folder or back to root
  - A one-time compatibility notice is shown before the first folder is ever created
- Resizable sidebar: drag handle between the sidebar and editor area lets you adjust sidebar width (150–520 px); chosen width is remembered across sessions (`feat(ui)`)
- Dialogs can now be dismissed by clicking on the backdrop outside the dialog box (`feat(ui)`)
- Batch color picker pre-selects the shared color when all selected notes have the same color, and toggles closed if the picker is already open (`feat(ui)`)

### Changed

- Editor title and action buttons wrap to a second row on narrow windows; responds dynamically to sidebar width changes without a fixed breakpoint (`feat(ui)`)

## [0.6.4] - 2026-05-28

### Fixed

- Windows: update check now retries up to 3 times on transient network errors (`fix(ui)`)
  - GitHub releases route through two CDN redirects to Azure Blob Storage; the Azure edge connection occasionally resets on Windows before a response arrives — retries work around this
  - "Check for Updates" button shows "Checking… (retry 2/3)" progress; startup toast retries silently in the background without blocking app launch
- Windows: `latest.json` is now served from GitHub Pages (Cloudflare CDN) as the primary updater endpoint, eliminating the Azure Blob Storage redirect chain (`fix(ci)`)
  - GitHub Releases URL kept as fallback
  - CI automatically deploys `latest.json` to Pages after each release via a new `deploy-pages` workflow job

## [0.6.3] - 2026-05-28

### Added

- Windows: startup update notification — a small toast in the bottom-right corner appears once at launch when a newer version is available, with an Install button (`feat(ui)`)
  - Toggle in Settings → Updates: "Show update notifications" (on by default)
  - Silent on network error at startup; the manual "Check for Updates" button still shows errors

### Fixed

- Windows: network errors during update check now show a readable message instead of a raw reqwest error (`fix(ui)`)
  - Signature mismatch errors (root cause of the v0.6.1→v0.6.2 update failure) now explicitly say "reinstall manually from GitHub" rather than crashing silently
  - Both updater commands now use a 30 s timeout via `updater_builder()` instead of the library default

## [0.6.2] - 2026-05-28

### Fixed

- Checklist: sidebar preview now applies the note's sort option (Unchecked First, Checked First, Alphabetical, Manual) instead of always sorting by raw order (`fix(checklist)`)
  - Applies the same sort logic as the editor, including Android-parity `originalOrder` tiebreaker for Manual mode
- Checklist: typing in an item after autosave no longer silently updates the wrong entry — server response no longer replaces the editor's live item list (`fix(checklist)`)
- Colored notes: selected-state highlight now uses a neutral darken/lighten tint (18% black / 20% white) instead of a fixed blue blend, keeping the note color visible under selection (`fix(ui)`)
- Notes list: relative timestamps ("5m ago", "2h ago") now refresh every 60 s in-place without triggering a full list re-render (no scroll-position resets) (`fix(ui)`)
- Windows: `latest.json` was missing from some releases because a stray newline in the signing-password env var caused `tauri-action` to skip the updater upload; fixed by switching from heredoc to direct assignment (`ci`)

## [0.6.1] - 2026-05-28

### Added

- Windows: in-app updater — "Check for Updates" button in Settings downloads and installs the latest release silently via NSIS passive mode; no admin rights required (`currentUser` install mode)
  - Update check and install driven by `tauri-plugin-updater`; Linux/macOS always returns "up to date" (package manager handles updates on those platforms)
  - Installer config: `installMode = currentUser` places the app in `%LOCALAPPDATA%\Programs`; settings in `%APPDATA%` survive an uninstall+reinstall cycle

### Fixed

- Linux: KDE Wayland taskbar shows generic icon instead of app icon — fixed by overriding the `xdg_toplevel` app_id to `simple-notes-desktop` via `gdk_wayland_window_set_application_id()` at window creation time
  - AUR: `.desktop` file renamed to `simple-notes-desktop.desktop` (freedesktop convention) to match the new app_id
  - `StartupWMClass` in the `.desktop` template corrected to `simple-notes-desktop` for X11 startup notification matching

## [0.6.0] - 2026-05-27

### Added

- Pinned notes — pinned notes always stay at the top of the list; sidebar shows "Pinned" / "Notes" section headers when any note is pinned; pin icon in the note meta line; batch Pin/Unpin buttons in the multi-select action bar (Android parity)
  - Backend: new `pin_notes` command sets `isPinned = Some(true)` or `None` (not `false` — Android-compatible); `NoteMetadata` carries `is_pinned` so the sidebar renders the icon without a full GET

- Note color picker — 11-color Keep-compatible palette matching Android v2.5.0; color swatch shown in the sidebar item; picker popup in the editor header; batch color change via Multi-Select
  - New `color_notes` backend command; `NoteMetadata` carries `color` for sidebar rendering
  - New `src/utils/ColorPicker.js` singleton popup; `src/utils/noteColors.js` palette with light/dark variants

- Note list sort selector — five sort modes matching Android: Last modified, Last created, Alphabetical A→Z, By note type, By color; plus Ascending/Descending direction toggle; preference persisted in `localStorage`
  - Sort button turns blue when a non-default option or direction is active; tooltip shows current sort + direction
  - `NoteMetadata` gains `created_at` field to support Last created sort without a full GET

- Checklist: Ctrl+Enter and a `+` button in the editor header as additional ways to add a new item (Android parity); bottom "+ Add Item" button delegates to the same `addChecklistItem()` method

### Fixed

- Checklist `MANUAL` sort now uses `originalOrder` for Android-parity — item order no longer drifts between apps after drag-drop or insert
  - `ChecklistItem` gains a first-class `original_order: Option<i32>` field (promoted out of the `extra` catch-all)
  - `_renumberOrders()` cements `order = originalOrder = i` after every structural change; `_fixPreF04Orders()` derives `originalOrder` on load for pre-v1.9.0 notes without the field (no save triggered)

- Window clamped to monitor bounds on first start so it never opens partially off-screen on multi-monitor or lower-resolution setups; window size persisted across restarts via `tauri-plugin-window-state`
  - Previous clamp ran in Rust before the Wayland surface was mapped (`outer_size()` returned 0×0); new JS clamp runs after DOM load with a 200 ms delay and accounts for OS titlebar height

- OS color-scheme changes now update the system theme live — no restart required when the system switches light/dark
  - `applyTheme('system')` now holds a persistent `MediaQueryList` and listens for `change` events

- Empty checklist items no longer reach the server on any save path (autosave timer, Ctrl+S, note-switch flush)
  - `_sanitizeForSave()` returns a cleaned shallow copy without mutating the note (keeps Undo snapshots intact)
  - Previous gap: `saveNoteImmediate()` had no empty-item guard; switching notes within the autosave debounce window could flush a blank item

- Per-note save queue prevents out-of-order writes — concurrent `saveNote()` calls for the same note now execute in submission order; saves for different notes still run concurrently

- Sort menu click-handler leak fixed — rapid open/close cycles no longer leave a stale `document` listener that consumes unrelated clicks

- Checklist blur timer guarded against note-switch — the 200 ms blur `setTimeout` now captures the note ID and item ID and bails if either changed before it fires; checkbox toggle skips `renderChecklist()` on MANUAL sort (preserves keyboard focus and scroll position)

- Stale save status no longer shown on the wrong note — `saveNoteImmediate()` now checks the active note ID before updating the sync-status indicator, so a late response from a previous note no longer flashes "Save error" on the newly opened note

- New checklist item always inserted as the last unchecked entry, not at array index 0 — a checked item at index 0 previously caused the new item to appear second after re-render

- Scroll position reset to top when switching notes — switching from a scrolled note no longer leaves the new note's editor/checklist/preview container scrolled to a mid-point

- Scroll buffer added when appending checklist items — new item is nudged 48 px into view so it doesn't sit flush against the viewport bottom edge

- Sync folder sanitization is now ASCII-only (matching the JS frontend regex) — Unicode letters such as é, ö were previously accepted by Rust but rejected by JS, silently producing mismatched server paths; `c.is_ascii_alphanumeric()` is now used on both sides

- Empty note title falls back to `untitled-{id[:8]}` for Markdown export; Markdown `PUT` errors propagated to the caller instead of only logged

- Failed note loads in `list_notes` logged to stderr instead of silently skipped — a single unreachable note no longer disappears from the list without any trace in app logs

- Poisoned Rust mutexes recovered gracefully — `lock_recover()` extracts the inner value instead of panicking; a panic while holding `WebDavState`/`DeviceIdState`/`TraySettings` no longer permanently breaks the affected lock

- Note list re-sorted in local cache immediately after save — color or pin changes are reflected without waiting for the next full sync

- Checklist item delete button always visible (raised resting opacity from 0 to 0.5; larger click target)

- Background save errors now shown in the sync-status line — `saveNoteImmediate()` previously only logged failures to the console

### Technical

- `save_settings` keys auto-derived from the `Settings` struct via `serde_json` serialization loop — adding a new field no longer requires a matching manual `store.set()` call; new `test_get_settings_keys_match_settings_struct` CI test catches struct/key drift
- Dead checklist/note helper methods removed from `models.rs`; PROPFIND and MKCOL as module-level `LazyLock` statics in `webdav.rs` (eliminates repeated `Method::from_bytes().unwrap()`)
- CI release body now shows only section headers and top-level bullets; sub-bullets and `**bold**` markers stripped by the awk/sed extractor

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

[Unreleased]: https://github.com/inventory69/simple-notes-desktop/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/inventory69/simple-notes-desktop/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/inventory69/simple-notes-desktop/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/inventory69/simple-notes-desktop/releases/tag/v0.1.0
