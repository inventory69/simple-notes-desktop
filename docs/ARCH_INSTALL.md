# Simple Notes Desktop - Arch Linux Installation

## Option 1: AppImage (Empfohlen - Keine Installation nötig)

```bash
# Download
wget https://github.com/inventory69/simple-notes-desktop/releases/download/v0.1.0/simple-notes-desktop_0.1.0_amd64.AppImage

# Ausführbar machen
chmod +x simple-notes-desktop_0.1.0_amd64.AppImage

# fuse2 installieren
sudo pacman -S fuse2

# Starten
./simple-notes-desktop_0.1.0_amd64.AppImage
```

**Desktop-Integration mit AppImageLauncher:**
```bash
yay -S appimagelauncher
# AppImage doppelklicken → "Integrate and run"
```

---

## Option 2: DEB mit debtap konvertieren

```bash
# debtap installieren
yay -S debtap
sudo debtap -u

# DEB herunterladen
wget https://github.com/inventory69/simple-notes-desktop/releases/download/v0.1.0/simple-notes-desktop_0.1.0_amd64.deb

# In Arch-Paket konvertieren
debtap simple-notes-desktop_0.1.0_amd64.deb

# Installieren
sudo pacman -U simple-notes-desktop-0.1.0-1-x86_64.pkg.tar.zst
```

---

## Option 3: AUR Package (Geplant)

Ein AUR-Package ist in Arbeit. Sobald es veröffentlicht ist:

```bash
yay -S simple-notes-desktop-bin
# oder
paru -S simple-notes-desktop-bin
```

### AUR Package selbst erstellen (für Maintainer)

```bash
# PKGBUILD ist bereits vorbereitet
cd aur/

# Package bauen
makepkg -si

# Checksum nach erstem Release aktualisieren:
# 1. Download .deb
# 2. sha256sum simple-notes-desktop_0.1.0_amd64.deb
# 3. In PKGBUILD sha256sums=('...') eintragen
```

**AUR Submission (nach erstem stabilen Release):**
```bash
# SSH-Key für AUR einrichten
ssh-keygen -t ed25519 -C "aur@archlinux.org"
# Public key auf https://aur.archlinux.org/account hochladen

# AUR Repo klonen
git clone ssh://aur@aur.archlinux.org/simple-notes-desktop-bin.git aur-repo
cd aur-repo

# Dateien kopieren
cp ../aur/PKGBUILD .
cp ../aur/README.md .

# .SRCINFO generieren
makepkg --printsrcinfo > .SRCINFO

# Committen und pushen
git add PKGBUILD .SRCINFO
git commit -m "Initial upload: v0.1.0"
git push
```

---

## Option 4: Aus Quellcode bauen

```bash
# Dependencies
sudo pacman -S nodejs pnpm rust webkit2gtk-4.1 libappindicator-gtk3 librsvg patchelf fuse2

# Repo klonen
git clone https://github.com/inventory69/simple-notes-desktop.git
cd simple-notes-desktop

# Bauen
pnpm install
pnpm build

# Binary finden
./src-tauri/target/release/bundle/appimage/simple-notes-desktop_0.1.0_amd64.AppImage
```

---

## Vergleich der Methoden

| Methode | Vorteile | Nachteile |
|---------|----------|-----------|
| **AppImage** | Keine Installation, portable, immer aktuell | Keine pacman-Integration |
| **debtap** | Volle pacman-Integration | Manuelle Updates |
| **AUR** | Automatische Updates, einfach | Noch nicht veröffentlicht |
| **Source Build** | Volle Kontrolle, neueste Version | Kompilierzeit, Dependencies |

**Empfehlung:** AppImage mit AppImageLauncher für beste Balance zwischen Einfachheit und Integration.
