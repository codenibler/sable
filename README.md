# Sable

Sable is a fully vibe-coded aggregated portfolio and net worth monitor, which stores data fully locally. Currently, it tracks Trading212, a hardware wallet cryptocurrency portfolio via Bitcoin XPUB and wallet adresses, and some other custom portfolios. If you like the look, you can always fork and vibe to your own financial preferences. 

Eventually, this might also become an iOS app, but don't quote me on that

https://github.com/user-attachments/assets/d3c770da-57f0-4e72-92e8-5d3cc05acb20

## Run locally

Requirements: Node.js, npm, Rust, and the Linux packages required by Tauri 2/WebKitGTK.

```sh
cp .env.example .env
npm install
npm run tauri dev

## Omarchy installation

Build the AppImage, copy it to `~/Applications/Sable.AppImage`, and install
`packaging/sable.desktop` under `~/.local/share/applications`. The launcher
uses this repository as its working directory so the ignored `.env` remains the
single credentials source; no secrets are copied into the Applications folder.

## Mobile

There is no iOS app to install. The desktop app serves a read-only progressive web app,
and the phone adds it to the Home Screen — one codebase, no Mac, no Apple developer account.

The desktop stays the single source of truth: it keeps the SQLite database and `.env`, and
the phone is a thin client of it. There is no second copy to synchronise, and no write
endpoint, so a leaked token cannot alter the ledger. Live data requires the desktop app to
be running; otherwise the phone shows its last snapshot with a "last updated" banner.

Enable the API in `.env`:

```sh
MOBILE_API_ENABLED=1
MOBILE_API_BIND=127.0.0.1:8787
MOBILE_API_TOKEN=$(openssl rand -hex 32)
```

The listener is loopback-only and refuses to start on a public address. Publish it to your
tailnet instead, which also supplies the HTTPS certificate iOS requires before it will
register the offline service worker:

```sh
sudo pacman -S tailscale
sudo systemctl enable --now tailscaled
sudo tailscale up
tailscale serve --bg 8787      # prints https://<machine>.<tailnet>.ts.net
```

Then install Tailscale on the phone, sign in to the same account, open that URL in Safari,
paste the token, and use Share → Add to Home Screen.

The PWA is served from the frontend bundle embedded in the binary, so it only exists in a
built app — `npm run tauri dev` points the webview at the Vite dev server and serves no
assets over HTTP. Rebuild after any frontend change:

```sh
npm run build && ./packaging/rebuild-and-install.sh
```

## Architecture

The React/TypeScript frontend contains no credentials and can only invoke a narrow set of Tauri commands. Rust owns environment loading, HTTP authentication, address validation, provider calls, aggregation, and SQLite access. The webview receives only display-ready portfolio data.

