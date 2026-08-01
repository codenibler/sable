# Portfolio 1

Portfolio 1 is a dark, local-first desktop monitor for a Trading 212 Invest account and grouped BTC, ETH, and SOL wallet addresses. It shows combined value in EUR, current holdings, source health, profit/loss, and an equity history built from local snapshots.

## Current scope

- Read-only Trading 212 account summary and open positions
- One automatically created crypto portfolio
- Any number of BTC, ETH, and SOL public addresses
- Native BTC, ETH, and SOL balances with EUR pricing
- Combined and crypto hourly snapshots in local SQLite
- Resumable Trading 212 cash-history backfill with net deposits, cash-flow-adjusted profit, and annualized MWRR
- Responsive React interface designed to carry forward to Tauri iOS
- Partial-failure handling when one provider is unavailable

Token contracts held by ETH or SOL wallets are not indexed yet. Public wallet addresses are safe to monitor, but Portfolio 1 will never ask for a private key or seed phrase. Configured RPC and explorer providers can see the public addresses requested from them; use self-hosted endpoints in `.env` if that metadata is sensitive.

## Run locally

Requirements: Node.js, npm, Rust, and the Linux packages required by Tauri 2/WebKitGTK.

```sh
cp .env.example .env
npm install
npm run tauri dev
```

Fill in `TRADING212_API_KEY` and `TRADING212_API_SECRET` using a key with only the account and portfolio read permissions required by the app. Never grant order permissions.

The existing `.env` is ignored by Git. Runtime endpoints, currency, timeouts, snapshot cadence, and the database filename are also configured there. The SQLite database is stored under the operating system's application-data directory, not in this repository.

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
