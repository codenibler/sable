# Portfolio 1

Portfolio 1 is a dark, local-first desktop monitor for a Trading 212 Invest account and BTC, ETH, and SOL wallets. It shows combined value in EUR, current holdings, source health, profit/loss, and an equity history built from local snapshots.

## Current scope

- Read-only Trading 212 account summary and open positions
- One automatically created crypto portfolio
- BTC public addresses and account-level mainnet `xpub`, `ypub`, or `zpub` keys
- Any number of ETH and SOL public addresses
- Native BTC, ETH, and SOL balances with EUR pricing
- Local manual investment sources, including Opessocius balance and net deposits from `.env`
- Combined and crypto hourly snapshots in local SQLite
- Resumable Trading 212 cash-history backfill with net deposits, cash-flow-adjusted profit, and annualized MWRR
- Responsive React interface designed to carry forward to Tauri iOS
- Partial-failure handling when one provider is unavailable

Token contracts held by ETH or SOL wallets are not indexed yet. Portfolio 1 will never ask for a private key or seed phrase. Extended public keys are sensitive metadata because they reveal a wallet's address history: keep `.env` private. XPUB derivation happens locally and Blockstream receives only the derived public addresses. All configured providers can still correlate addresses requested from the same connection; use self-hosted endpoints in `.env` if that metadata is sensitive.

## Run locally

Requirements: Node.js, npm, Rust, and the Linux packages required by Tauri 2/WebKitGTK.

```sh
cp .env.example .env
npm install
npm run tauri dev
```

Fill in `TRADING212_API_KEY` and `TRADING212_API_SECRET` using a key with only the account and portfolio read permissions required by the app. Never grant order permissions.

The existing `.env` is ignored by Git. Wallets can be imported at startup with `HWR_BITCOIN_XPUBS`, `HWR_ETHEREUM_ADDRESSES`, and `HWR_SOLANA_ADDRESSES`; comma-separated entries are accepted and repeated starts do not duplicate them. Opessocius is updated locally through `OPESSOCIUS_CURRENT_BALANCE` and `OPESSOCIUS_NET_DEPOSITS` and never makes a network request. Runtime endpoints, currency, timeouts, snapshot cadence, XPUB scan limits, and the database filename are also configured there. The SQLite database is stored under the operating system's application-data directory, not in this repository.

## Verification

```sh
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

## Data providers

- [Trading 212 Public API](https://docs.trading212.com/api) for the Invest account
- [CoinGecko](https://docs.coingecko.com/reference/simple-price) for native-asset EUR prices
- [Blockstream Esplora](https://github.com/Blockstream/esplora/blob/master/API.md) for BTC address balances
- [Ethereum JSON-RPC](https://ethereum.org/developers/docs/apis/json-rpc/#eth_getbalance) for ETH balances
- [Solana JSON-RPC](https://solana.com/docs/rpc/http/getbalance) for SOL balances
- [Frankfurter](https://frankfurter.dev/) for non-EUR brokerage conversion

All provider base URLs are replaceable in `.env`, so public endpoints can later be swapped for private nodes or authenticated services without changing application code.

## Architecture

The React/TypeScript frontend contains no credentials and can only invoke a narrow set of Tauri commands. Rust owns environment loading, HTTP authentication, address validation, provider calls, aggregation, and SQLite access. The webview receives only display-ready portfolio data.

Trading 212 history is retrieved in rate-safe batches. Large histories continue automatically while the app remains open, and manual refreshes resume from the same stored cursor; already stored events are never duplicated.

The next logical milestones are token indexing, historical market-value reconstruction before the first local snapshot, operating-system keyring storage, background refresh, Linux packaging, and then Tauri's iOS target with an explicit synchronization design.
