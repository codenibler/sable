# Sable

Sable is a dark, local-first desktop monitor for a Trading 212 Invest account and BTC, ETH, LINK, and SOL wallets. It shows combined value in EUR, current holdings, source health, profit/loss, and an equity history built from local snapshots.

The interface uses [General Sans](https://www.fontshare.com/fonts/general-sans) by Indian Type Foundry, bundled locally under the [Fontshare free-font license](https://www.fontshare.com/licenses/itf-ffl).

## Current scope

- Read-only Trading 212 account summary and open positions
- Private local Net Worth ledger with dated balance categories, bar-chart progression, growth analytics, and editable snapshots
- One automatically created Trezor Safe crypto portfolio
- BTC public addresses and account-level mainnet `xpub`, `ypub`, or `zpub` keys
- Any number of ETH and SOL public addresses
- Native BTC, ETH, and SOL balances plus configured Ethereum ERC-20 tokens with EUR pricing
- Separate ETH and LINK balances, allocation, and performance for the same Ethereum address
- Private local Opessocius monthly history with automatic future returns and editable overrides
- Clickable per-portfolio equity charts and metrics for Trading 212, Opessocius, and Trezor Safe
- Combined and crypto hourly snapshots in local SQLite
- Resumable Trading 212 cash-history backfill with net deposits and simple total return
- Responsive React interface designed to carry forward to Tauri iOS
- Partial-failure handling when one provider is unavailable

Configured Ethereum token contracts are treated as holdings of the same Trezor Safe as their owning address. Solana tokens are not indexed yet. Sable will never ask for a private key or seed phrase. Extended public keys are sensitive metadata because they reveal a wallet's address history: keep `.env` private. XPUB derivation happens locally and Blockstream receives only the derived public addresses. All configured providers can still correlate addresses requested from the same connection; use self-hosted endpoints in `.env` if that metadata is sensitive.

## Run locally

Requirements: Node.js, npm, Rust, and the Linux packages required by Tauri 2/WebKitGTK.

```sh
cp .env.example .env
npm install
npm run tauri dev
```

Fill in `TRADING212_API_KEY` and `TRADING212_API_SECRET` using a key with only the account and portfolio read permissions required by the app. Never grant order permissions.

The existing `.env` is ignored by Git. Wallets can be imported at startup with `HWR_BITCOIN_XPUBS`, `HWR_ETHEREUM_ADDRESSES`, and `HWR_SOLANA_ADDRESSES`; comma-separated entries are accepted and repeated starts do not duplicate them. `ETHEREUM_ERC20_TOKENS` defines optional Ethereum tokens as semicolon-separated `CoinGecko ID|symbol|name|contract|decimals` records; LINK is configured by default. `NET_WORTH_HISTORY_FILE` points to an ignored local CSV that is imported once and kept in sync as a portable ledger mirror. Opessocius uses `OPESSOCIUS_CURRENT_BALANCE` and `OPESSOCIUS_NET_DEPOSITS` as its baseline and never makes a network request. `OPESSOCIUS_HISTORY_FILE` points to an ignored local CSV containing the authoritative monthly ledger through July 2026, so private figures are not committed. Automatic returns begin at `OPESSOCIUS_RETURN_START_MONTH` (August 2026 by default). On each month's final calendar day, `OPESSOCIUS_MONTHLY_RETURN_RATE` is applied to that month's opening balance (2% by default); shorter months use their actual final day, and missed completed months are compounded in sequence. The UI can override the latest completed automatic return, including with a negative amount. Overrides replace the default and are distributed across that month for chart and period-return calculations. Runtime endpoints, currency, timeouts, snapshot cadence, XPUB scan limits, and the database filename are also configured there. The SQLite database is stored under the operating system's application-data directory, not in this repository. Sable creates a private full database backup at each launch and retains the newest `DATABASE_BACKUP_COUNT` copies (seven by default) beside the database. On Unix systems, credentials, private CSVs, the database, and backups are restricted to the current user.

## Verification

```sh
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

## Omarchy installation

Build the AppImage, copy it to `~/Applications/Sable.AppImage`, and install
`packaging/sable.desktop` under `~/.local/share/applications`. The launcher
uses this repository as its working directory so the ignored `.env` remains the
single credentials source; no secrets are copied into the Applications folder.

## Data providers

- [Trading 212 Public API](https://docs.trading212.com/api) for the Invest account
- [CoinGecko](https://docs.coingecko.com/reference/simple-price) for native-asset EUR prices
- [Blockstream Esplora](https://github.com/Blockstream/esplora/blob/master/API.md) for BTC address balances
- [Ethereum JSON-RPC](https://ethereum.org/developers/docs/apis/json-rpc/) for native ETH and configured ERC-20 balances
- [Solana JSON-RPC](https://solana.com/docs/rpc/http/getbalance) for SOL balances
- [Frankfurter](https://frankfurter.dev/) for non-EUR brokerage conversion

All provider base URLs are replaceable in `.env`, so public endpoints can later be swapped for private nodes or authenticated services without changing application code.

## Architecture

The React/TypeScript frontend contains no credentials and can only invoke a narrow set of Tauri commands. Rust owns environment loading, HTTP authentication, address validation, provider calls, aggregation, and SQLite access. The webview receives only display-ready portfolio data.

Trading 212 history is retrieved in rate-safe batches. Large histories continue automatically while the app remains open, and manual refreshes resume from the same stored cursor; already stored events are never duplicated.

The next logical milestones are Solana token indexing, historical market-value reconstruction before the first local snapshot, operating-system keyring storage, Linux packaging, and then Tauri's iOS target with an explicit synchronization design.
