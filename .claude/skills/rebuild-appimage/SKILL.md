---
name: rebuild-appimage
description: >
  Rebuild the Sable AppImage and install it over ~/Applications/Sable.AppImage.
  Use whenever the user asks to rebuild, repackage, reinstall, or ship the app,
  or to see a change land in the installed desktop app rather than `npm run tauri dev`.
  Covers the NO_STRIP workaround that a plain `npm run tauri build` fails on.
---

# Rebuild the Sable AppImage

One shot, from the repository root:

```sh
./packaging/rebuild-and-install.sh
```

That builds the AppImage and refreshes the launcher entry ([[install-launcher]]
documents the launcher half on its own). Takes roughly a minute; the release
Rust build dominates.

## Doing it by hand

```sh
NO_STRIP=1 npm run tauri build -- --bundles appimage
cp src-tauri/target/release/bundle/appimage/Sable_0.1.0_amd64.AppImage ~/Applications/Sable.AppImage
```

Drop `--bundles appimage` to also produce the `.deb` and `.rpm` — nothing in this
setup consumes them, so skip it unless asked.

## NO_STRIP is required

Without `NO_STRIP=1` the bundle step fails with:

```
failed to bundle project: `failed to run linuxdeploy`
```

`npm run tauri build` swallows the cause; re-run with `--verbose` to see it. The
`linuxdeploy` AppImage in `~/.cache/tauri/` carries a binutils `strip` too old to
read the `.relr.dyn` sections in current Arch system libraries, so it errors on
every bundled `.so` and takes the bundle down with it. `NO_STRIP=1` skips
stripping and only leaves debug symbols in those libraries.

## Verifying the build

```sh
SABLE_ENV_FILE=$PWD/.env timeout 12 ~/Applications/Sable.AppImage
```

Exit code 124 means it was still running when the timeout killed it — that is the
healthy result. A window opens for those seconds.

Launching also runs the SQLite migrations in `src-tauri/src/db.rs` against the
real database at `~/.local/share/app.sable/portfolio-1.sqlite3` and rewrites
`data/net-worth-history.csv` from it. Both are backed up automatically on
startup (`~/.local/share/app.sable/backups/`, 7 retained), but say so before
running when a migration is part of the change.
