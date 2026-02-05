# Building Simple Notes Desktop

Diese Anleitung beschreibt, wie du Simple Notes Desktop lokal oder automatisiert bauen kannst.

## Voraussetzungen

### Alle Plattformen
- **Node.js** 20+
- **pnpm** 9+
- **Rust** (stable)

### Linux (Debian/Ubuntu)
```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  patchelf \
  libfuse2
```

### Linux (Arch)
```bash
sudo pacman -S webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf fuse2
# Für AppImage-Erstellung:
paru -S appimagetool-bin  # oder aus AUR
```

### Windows
- Visual Studio Build Tools 2019+ mit C++ Workload
- WebView2 Runtime (Windows 10/11 hat es standardmäßig)

---

## Lokales Entwickeln

```bash
# Dependencies installieren
pnpm install

# Development-Modus starten
pnpm dev
```

Der `pnpm dev` Befehl setzt automatisch die notwendigen WebKit-Umgebungsvariablen für Linux/Wayland:
- `WEBKIT_DISABLE_DMABUF_RENDERER=1`
- `WEBKIT_DISABLE_COMPOSITING_MODE=1`

---

## Manuell Bauen

### Standard Build (empfohlen)

```bash
pnpm build
```

Das erstellt alle verfügbaren Pakete für deine Plattform:

| Plattform | Ausgabe |
|-----------|---------|
| Linux | `.deb`, `.rpm` |
| Windows | `.msi`, `.exe` |

Die Dateien findest du unter:
```
src-tauri/target/release/bundle/
├── deb/
│   └── Simple Notes Desktop_0.1.0_amd64.deb
├── rpm/
│   └── Simple Notes Desktop-0.1.0-1.x86_64.rpm
└── msi/ / nsis/
    └── Simple Notes Desktop_0.1.0_x64-setup.exe
```

### Raw Build (ohne Wrapper)

Falls du den Build ohne das Wrapper-Script starten willst:

```bash
# Nur Tauri build
pnpm build:raw

# Oder direkt
pnpm tauri build
```

⚠️ **Achtung:** Ohne Wrapper werden die WebKit-Fixes auf Linux nicht gesetzt!

### Nur bestimmte Targets

```bash
# Nur DEB
pnpm tauri build --bundles deb

# Nur RPM
pnpm tauri build --bundles rpm
```

---

## Linux Installation (lokal gebaut)

Nach dem Build kannst du die Pakete direkt installieren:

```bash
# Debian/Ubuntu
sudo apt install ./src-tauri/target/release/bundle/deb/*.deb

# Fedora/RHEL/CentOS
sudo dnf install ./src-tauri/target/release/bundle/rpm/*.rpm
```

Alle Abhängigkeiten werden automatisch durch den Package Manager aufgelöst.

---

## Warum kein AppImage?

AppImage ist auf vielen Systemen (besonders Arch Linux) problematisch wegen `linuxdeploy` Inkompatibilität mit dem `strip`-Tool. Die DEB/RPM Pakete sind:
- ✅ Kleiner (~4 MB statt 96 MB)
- ✅ Saubere Dependency-Auflösung
- ✅ Native Integration in den Package Manager
- ✅ Einfacher zu warten

---

## Automatisierter Build (GitHub Actions)

### Manueller Workflow-Trigger

1. Gehe zu **Actions** → **Build & Release**
2. Klicke **Run workflow**
3. Optional: "Create release" aktivieren

### Release erstellen

```bash
# Tag erstellen und pushen
git tag v0.1.0
git push origin v0.1.0
```

Dies startet automatisch:
1. Build für alle Plattformen (Linux, macOS Intel/ARM, Windows)
2. Erstellt einen Release-Draft mit allen Artifacts

### Workflow-Features

- **Multi-Platform:** Baut für Linux, macOS (Intel + Apple Silicon), Windows
- **Caching:** pnpm store und Rust target werden gecached
- **Auto-Release:** Bei Tags wird automatisch ein Release-Draft erstellt
- **Artifact-Upload:** Bei manuellen Runs werden Artifacts 7 Tage gespeichert

---

## Troubleshooting

### Linux: AppImage startet nicht

```bash
# fuse2 installieren
sudo pacman -S fuse2  # Arch
sudo apt install libfuse2  # Debian/Ubuntu
```

### macOS: "App ist beschädigt"

```bash
xattr -cr "Simple Notes Desktop.app"
```

### Windows: WebView2 fehlt

Lade den [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) herunter.

---

## Versionierung

Die Version muss an **drei Stellen** konsistent sein:

1. `package.json` → `"version": "0.1.0"`
2. `src-tauri/tauri.conf.json` → `"version": "0.1.0"`
3. `src-tauri/Cargo.toml` → `version = "0.1.0"`

Bei einem Release:
```bash
# Version überall ändern, dann:
git add -A
git commit -m "chore: bump version to 0.2.0"
git tag v0.2.0
git push && git push --tags
```

---

## Entwicklungs-Workflow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  pnpm dev   │ ──▶ │   Testen    │ ──▶ │ pnpm build  │
└─────────────┘     └─────────────┘     └─────────────┘
                                               │
                                               ▼
                                        ┌─────────────┐
                                        │  git tag    │
                                        │  git push   │
                                        └─────────────┘
                                               │
                                               ▼
                                        ┌─────────────┐
                                        │  GitHub     │
                                        │  Actions    │
                                        └─────────────┘
                                               │
                                               ▼
                                        ┌─────────────┐
                                        │  Release    │
                                        │  Draft      │
                                        └─────────────┘
```
