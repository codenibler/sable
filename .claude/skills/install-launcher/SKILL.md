---
name: install-launcher
description: >
  Install or refresh Sable's Omarchy desktop launcher entry — the navbar/app-menu
  item, its icon, and the env the app starts with. Use when the user asks to update
  the navbar, launcher, app menu, taskbar entry, or desktop icon, or after editing
  packaging/sable.desktop or src-tauri/icons/icon.png.
---

# Install the desktop launcher entry

```sh
install -Dm644 packaging/sable.desktop ~/.local/share/applications/sable.desktop
install -Dm644 src-tauri/icons/icon.png ~/.local/share/icons/hicolor/512x512/apps/sable.png
update-desktop-database ~/.local/share/applications
gtk-update-icon-cache -f -t ~/.local/share/icons/hicolor
```

`packaging/rebuild-and-install.sh` already does this after building, so run these
separately only when the binary itself has not changed. Rebuilding the app is
[[rebuild-appimage]].

## What the entry does

`packaging/sable.desktop` is the source of truth — edit it in the repository and
reinstall, never edit the copy under `~/.local/share/applications/`.

It launches `~/Applications/Sable.AppImage` through `env SABLE_ENV_FILE=<repo>/.env`
and sets `Path=` to the repository. That indirection is deliberate: the gitignored
`.env` stays the single source of credentials and nothing secret is copied into
`~/Applications`. Keep both absolute paths pointing at the checkout if the
repository ever moves.

`StartupWMClass=sable` matches the window class the Tauri binary reports, which is
what lets Hyprland associate the window with this launcher. It must stay in sync
with the binary name in `src-tauri/Cargo.toml`, not with `productName`.

## Checking it took

```sh
grep -c . ~/.local/share/applications/sable.desktop   # entry is installed
gio launch ~/.local/share/applications/sable.desktop  # launches exactly as the menu would
```

The entry shows up in Walker/the app menu immediately; no compositor restart is
needed.
