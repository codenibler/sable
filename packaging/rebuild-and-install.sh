#!/usr/bin/env bash
# Rebuild the Sable AppImage and refresh the Omarchy launcher entry in one shot.
# Run from anywhere; paths resolve relative to this repository.
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository"

# linuxdeploy ships an old `strip` that cannot read the `.relr.dyn` sections in current
# Arch system libraries, and the whole bundle step fails on it. NO_STRIP skips stripping;
# it only leaves debug symbols in the bundled libraries.
NO_STRIP=1 npm run tauri build -- --bundles appimage

install -Dm755 "src-tauri/target/release/bundle/appimage/Sable_0.1.0_amd64.AppImage" \
    "$HOME/Applications/Sable.AppImage"
install -Dm644 packaging/sable.desktop "$HOME/.local/share/applications/sable.desktop"
install -Dm644 src-tauri/icons/icon.png \
    "$HOME/.local/share/icons/hicolor/512x512/apps/sable.png"
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "Installed $HOME/Applications/Sable.AppImage and refreshed the launcher entry."
