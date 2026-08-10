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

It runs `packaging/sable-launch` rather than the AppImage, and sets `Path=` to the
repository. Going through the wrapper is what makes a menu click reveal the running
instance instead of stacking up a new one behind the `special:sable` window rule.

The entry itself no longer sets any env. The wrapper resolves both
`packaging/sable-monitor` and the env file from its own location, so it starts Sable
with `SABLE_ENV_FILE=<repo>/.env` derived from wherever the script lives — the
gitignored `.env` stays the single source of credentials and nothing secret is copied
into `~/Applications`. An inherited `SABLE_ENV_FILE` still wins, and `sable-monitor`'s
own hardcoded `~/Projects/sable/.env` default only applies when it is run directly.
Keep the absolute paths in the `.desktop` entry pointing at the checkout if the
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
