use std::env;

#[derive(Clone)]
pub struct Config {
    pub trading212_api_key: Option<String>,
    pub trading212_api_secret: Option<String>,
    pub trading212_base_url: String,
    pub coingecko_base_url: String,
    pub blockstream_base_url: String,
    pub ethereum_rpc_url: String,
    pub solana_rpc_url: String,
    pub frankfurter_base_url: String,
    pub base_currency: String,
    pub snapshot_interval_minutes: i64,
    pub http_timeout_seconds: u64,
    pub database_filename: String,
    pub trading212_history_max_pages: usize,
    pub history_sync_interval_minutes: i64,
    pub history_backfill_retry_seconds: u64,
    pub xpub_gap_limit: usize,
    pub xpub_scan_concurrency: usize,
    pub xpub_max_addresses_per_branch: usize,
    pub xpub_refresh_interval_minutes: i64,
    pub opessocius_name: String,
    pub opessocius_current_balance: f64,
    pub opessocius_net_deposits: f64,
    pub configured_bitcoin_xpubs: Vec<String>,
    pub configured_ethereum_addresses: Vec<String>,
    pub configured_solana_addresses: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        load_dotenv();

        Ok(Self {
            trading212_api_key: optional("TRADING212_API_KEY"),
            trading212_api_secret: optional("TRADING212_API_SECRET"),
            trading212_base_url: required("TRADING212_BASE_URL")?,
            coingecko_base_url: required("COINGECKO_BASE_URL")?,
            blockstream_base_url: required("BLOCKSTREAM_BASE_URL")?,
            ethereum_rpc_url: required("ETHEREUM_RPC_URL")?,
            solana_rpc_url: required("SOLANA_RPC_URL")?,
            frankfurter_base_url: required("FRANKFURTER_BASE_URL")?,
            base_currency: required("APP_BASE_CURRENCY")?.to_uppercase(),
            snapshot_interval_minutes: required("SNAPSHOT_INTERVAL_MINUTES")?
                .parse()
                .map_err(|_| "SNAPSHOT_INTERVAL_MINUTES must be a whole number".to_string())?,
            http_timeout_seconds: required("HTTP_TIMEOUT_SECONDS")?
                .parse()
                .map_err(|_| "HTTP_TIMEOUT_SECONDS must be a whole number".to_string())?,
            database_filename: required("DATABASE_FILENAME")?,
            trading212_history_max_pages: required("TRADING212_HISTORY_MAX_PAGES")?
                .parse::<usize>()
                .map_err(|_| "TRADING212_HISTORY_MAX_PAGES must be a whole number".to_string())?
                .clamp(1, 5),
            history_sync_interval_minutes: required("HISTORY_SYNC_INTERVAL_MINUTES")?
                .parse()
                .map_err(|_| "HISTORY_SYNC_INTERVAL_MINUTES must be a whole number".to_string())?,
            history_backfill_retry_seconds: required("HISTORY_BACKFILL_RETRY_SECONDS")?
                .parse()
                .map_err(|_| "HISTORY_BACKFILL_RETRY_SECONDS must be a whole number".to_string())?,
            xpub_gap_limit: whole_number("XPUB_GAP_LIMIT")?.clamp(1, 100),
            xpub_scan_concurrency: whole_number("XPUB_SCAN_CONCURRENCY")?.clamp(1, 16),
            xpub_max_addresses_per_branch: whole_number("XPUB_MAX_ADDRESSES_PER_BRANCH")?
                .clamp(20, 5_000),
            xpub_refresh_interval_minutes: whole_number("XPUB_REFRESH_INTERVAL_MINUTES")?
                .clamp(1, 10_080) as i64,
            opessocius_name: required("OPESSOCIUS_NAME")?,
            opessocius_current_balance: non_negative_amount("OPESSOCIUS_CURRENT_BALANCE")?,
            opessocius_net_deposits: non_negative_amount("OPESSOCIUS_NET_DEPOSITS")?,
            configured_bitcoin_xpubs: configured_list("HWR_BITCOIN_XPUBS"),
            configured_ethereum_addresses: configured_list("HWR_ETHEREUM_ADDRESSES"),
            configured_solana_addresses: configured_list("HWR_SOLANA_ADDRESSES"),
        })
    }

    pub fn trading212_is_configured(&self) -> bool {
        self.trading212_api_key.is_some() && self.trading212_api_secret.is_some()
    }
}

fn load_dotenv() {
    if dotenvy::dotenv().is_ok() {
        return;
    }
    let Ok(executable) = env::current_exe() else {
        return;
    };
    if let Some(path) = executable
        .ancestors()
        .skip(1)
        .map(|directory| directory.join(".env"))
        .find(|candidate| candidate.is_file())
    {
        let _ = dotenvy::from_path(path);
    }
}

fn optional(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn required(key: &str) -> Result<String, String> {
    optional(key).ok_or_else(|| format!("Missing required configuration: {key}"))
}

fn whole_number(key: &str) -> Result<usize, String> {
    required(key)?
        .parse()
        .map_err(|_| format!("{key} must be a whole number"))
}

fn non_negative_amount(key: &str) -> Result<f64, String> {
    let amount = required(key)?
        .parse::<f64>()
        .map_err(|_| format!("{key} must be a valid amount"))?;
    if amount.is_finite() && amount >= 0.0 {
        Ok(amount)
    } else {
        Err(format!("{key} must be a non-negative amount"))
    }
}

fn configured_list(key: &str) -> Vec<String> {
    optional(key)
        .map(|value| {
            value
                .split(|character: char| {
                    character == ',' || character == ';' || character.is_whitespace()
                })
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
