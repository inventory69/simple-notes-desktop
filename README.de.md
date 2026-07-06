<div align="center">
<img src="src-tauri/icons/128x128.png" alt="Simple Notes Desktop" width="128" />
</div>

<h1 align="center">Simple Notes Desktop</h1>

<h4 align="center">Local-first Notizen-App mit WebDAV-Sync — der Desktop-Begleiter zu Simple Notes Sync.</h4>

<div align="center">

[![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)](#-download)
[![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)](#-download)
[![Tauri](https://img.shields.io/badge/Tauri_2.0-24C8DB?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app/)
[![License](https://img.shields.io/badge/License-MIT-F5C400?style=for-the-badge)](LICENSE)

</div>

<div align="center">

[📥 Download](#-download) · [📖 Dokumentation](#-dokumentation) · [🤝 Mitmachen](CONTRIBUTING.md)

**🌍** Deutsch · [English](README.md)

</div>

---

## 📥 Download

Lade das passende Paket für deine Plattform herunter:

| Plattform | Download | Format |
|-----------|----------|--------|
| **Windows** | [Download](https://github.com/inventory69/simple-notes-desktop/releases/latest) | `.msi` / `.exe` |
| **Linux (Debian/Ubuntu)** | [Download](https://github.com/inventory69/simple-notes-desktop/releases/latest) | `.deb` |
| **Linux (Fedora/RHEL)** | [Download](https://github.com/inventory69/simple-notes-desktop/releases/latest) | `.rpm` |
| **Arch Linux** | [Installationsanleitung](docs/ARCH_INSTALL.md) | AUR / AppImage |

Windows-Installationen aktualisieren sich über den eingebauten Updater. Unter Linux aktualisierst du über deinen Paketmanager oder das neueste Release.

---

## 📱 Screenshots

<p align="center">
  <img src="screenshots/note_with_editmode.png" width="700" alt="Notiz-Editor mit Markdown-Vorschau">
</p>

<p align="center">
  <img src="screenshots/checklist.png" height="380" alt="Checklisten-Ansicht">
  <img src="screenshots/settings1.png" height="380" alt="Einstellungen">
</p>

<div align="center">

📝 Markdown-Editor &nbsp;•&nbsp; ✅ Checklisten &nbsp;•&nbsp; 📁 Ordner &nbsp;•&nbsp; 🎨 15 Themes &nbsp;•&nbsp; 🔄 WebDAV-Sync

</div>

---

## ✨ Highlights

- 🗄️ **Local-first & offline** — Notizen liegen lokal und sind mit oder ohne Server sofort bearbeitbar; die WebDAV-Sync läuft im Hintergrund, sobald verbunden
- 📝 **Markdown-Editor** — Syntax-Highlighting, Formatierungs-Toolbar und Live-Vorschau (CodeMirror 6)
- ✅ **Checklisten** — Tap-to-Check, Drag-to-Reorder, 5 Sortiermodi, Trennlinie zwischen offen/erledigt
- 📁 **Ordner** — Notizen in Ordner sortieren; einen Ordner **nur-lokal** markieren, um ihn vom Server fernzuhalten
- 🗑️ **Papierkorb** — Soft-Delete mit Wiederherstellen, endgültigem Löschen und geräteübergreifender Lösch-Sync
- 📌 **Anpinnen, Farbe & Sortierung** — Notizen oben anheften, Keep-kompatible Farben vergeben, Liste auf fünf Arten sortieren
- 🔀 **Mehrfachauswahl** — Notizen gebündelt anpinnen, färben, verschieben oder löschen (F6)
- 🎨 **15 Themes** — Breeze, Catppuccin, Nord, Gruvbox, Tokyo Night, Rosé Pine u. v. m. — plus System/Hell/Dunkel
- 🔄 **WebDAV-Sync** — Funktioniert mit Nextcloud, dem Simple-Notes-Server und jedem WebDAV-Anbieter
- 🔒 **Lokale Server** — Verbinde dich mit `localhost` und privaten IPs, die Browser-PWAs nicht erreichen
- 📄 **Markdown-Export** — Lesbare `.md`-Kopien werden neben dem JSON auf dem Server abgelegt
- 🔍 **Suche** — Notizen nach Titel oder Inhalt filtern, während du tippst
- 🖥️ **Nativer Desktop** — System-Tray, Autostart, verstellbare Seitenleiste und (unter Windows) ein In-App-Updater

---

## 🔗 Simple Notes Ökosystem

Diese App ist Teil der **Simple Notes** Familie — alle Apps nutzen das gleiche Datenformat und synchronisieren nahtlos:

| App | Plattform | Beschreibung |
|-----|-----------|--------------|
| [**Simple Notes Sync**](https://github.com/inventory69/simple-notes-sync) | Android | Mobile App mit Offline-first Sync |
| **Simple Notes Desktop** | Windows/Linux | Du bist hier! Native Desktop-Erfahrung |

### Warum Desktop?

Die Desktop-App löst ein kritisches Problem: **Lokale WebDAV-Server** (localhost, private IPs wie `192.168.x.x`, einfaches `http://`) können von browser-basierten PWAs nicht erreicht werden aufgrund von:
- Mixed Content (HTTPS → HTTP) Blocking
- CORS-Einschränkungen

Simple Notes Desktop nutzt native HTTP-Requests und umgeht diese Browser-Einschränkungen.

---

## 🚀 Schnellstart

### 1. Download & Installation

Lade das passende Paket für deine Plattform von der [Releases](https://github.com/inventory69/simple-notes-desktop/releases/latest) Seite herunter und installiere es. Die App startet direkt in ein funktionierendes, **offline** nutzbares Notizbuch — kein Konto, kein Server nötig.

### 2. (Optional) WebDAV-Server einrichten

Sync brauchst du nur, wenn deine Notizen auf mehreren Geräten liegen sollen.

**Option A — Simple Notes Server (Docker)**

```bash
git clone https://github.com/inventory69/simple-notes-sync.git
cd simple-notes-sync/server
cp .env.example .env
# Bearbeite .env und setze dein Passwort
docker compose up -d
```

**Option B — Deine bestehende Nextcloud**

```
https://deine-nextcloud.de/remote.php/dav/files/BENUTZERNAME/Notes/
```

### 3. Verbinden

1. Öffne die **Einstellungen** (⚙️)
2. Schalte den **Offline-Modus** aus und gib WebDAV-URL, Benutzername und Passwort ein
3. Klicke **Verbindung testen**, dann **Speichern**
4. Deine Notizen synchronisieren sich automatisch im Hintergrund 🎉

➡️ **Detaillierte Anleitung:** [docs/SETUP.md](docs/SETUP.md)

---

## ⌨️ Tastenkürzel

| Kürzel | Aktion |
|--------|--------|
| `Ctrl+N` | Neue Notiz |
| `Ctrl+Shift+N` | Neue Checkliste |
| `Ctrl+S` | Sofort speichern |
| `Ctrl+F` | Notizen suchen |
| `Ctrl+B` / `Ctrl+I` | Fett / kursiv (Editor) |
| `Ctrl+Z` | Rückgängig |
| `F6` | Mehrfachauswahl umschalten |
| `Esc` | Dialog schließen / Suche leeren |

---

## 📚 Dokumentation

| Dokument | Beschreibung |
|----------|--------------|
| [SETUP.md](docs/SETUP.md) | Detaillierte Installation & Konfiguration |
| [BUILDING.md](BUILDING.md) | Aus Quellcode bauen (Entwickler) |
| [CHANGELOG.md](CHANGELOG.md) | Versionsgeschichte |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Entwicklungs-Setup & Konventionen |

---

## 🔧 Problemlösungen

### Linux: AppImage startet nicht

Installiere fuse2 (benötigt für AppImage):
```bash
# Arch
sudo pacman -S fuse2

# Debian/Ubuntu
sudo apt install libfuse2
```

---

## 🤝 Mitmachen

Beiträge sind willkommen! Lies [CONTRIBUTING.md](CONTRIBUTING.md) für Richtlinien.

```bash
# Repository klonen
git clone https://github.com/inventory69/simple-notes-desktop.git
cd simple-notes-desktop

# Abhängigkeiten installieren (pnpm erforderlich)
pnpm install

# Development-Server starten
pnpm dev

# Für Produktion bauen
pnpm build
```

---

## 📄 Lizenz

MIT-Lizenz — siehe [LICENSE](LICENSE)

---

<div align="center">

**v0.10.0** · Mit ❤️ gebaut mit [Tauri](https://tauri.app/) + [CodeMirror](https://codemirror.net/)

</div>
