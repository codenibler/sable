# Sable

Sable is a fully vibe-coded aggregated portfolio and net worth monitor, which stores data fully locally. Currently, it tracks Trading212, a hardware wallet cryptocurrency portfolio via Bitcoin XPUB and wallet adresses, and some other custom portfolios. If you like the look, you can always fork and vibe to your own financial preferences. 

Eventually, this might also become an iOS app, but don't quote me on that

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

## Architecture

The React/TypeScript frontend contains no credentials and can only invoke a narrow set of Tauri commands. Rust owns environment loading, HTTP authentication, address validation, provider calls, aggregation, and SQLite access. The webview receives only display-ready portfolio data.

