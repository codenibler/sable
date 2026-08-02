use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use chrono::{Datelike, NaiveDate};

use crate::models::{NetWorthEntry, SaveNetWorthInput};

#[derive(Clone, Debug)]
pub struct OpessociusHistoryRow {
    pub month: String,
    pub return_percent: f64,
    pub return_eur: f64,
    pub deposits_eur: f64,
    pub withdrawals_eur: f64,
    pub ending_balance_eur: f64,
}

#[derive(Clone, Debug)]
pub struct EthereumTokenConfig {
    pub price_id: String,
    pub symbol: String,
    pub name: String,
    pub contract_address: String,
    pub decimals: u32,
}

#[derive(Clone)]
pub struct Config {
    pub trading212_api_key: Option<String>,
    pub trading212_api_secret: Option<String>,
    pub trading212_base_url: String,
    pub coingecko_base_url: String,
    pub blockstream_base_url: String,
    pub ethereum_rpc_url: String,
    pub ethereum_tokens: Vec<EthereumTokenConfig>,
    pub solana_rpc_url: String,
    pub frankfurter_base_url: String,
    pub base_currency: String,
    pub snapshot_interval_minutes: i64,
    pub http_timeout_seconds: u64,
    pub database_filename: String,
    pub database_backup_count: usize,
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
    pub opessocius_monthly_return_rate: f64,
    pub opessocius_return_start_month: String,
    pub opessocius_history: Vec<OpessociusHistoryRow>,
    pub net_worth_history: Vec<SaveNetWorthInput>,
    pub net_worth_history_path: Option<PathBuf>,
    pub configured_bitcoin_xpubs: Vec<String>,
    pub configured_ethereum_addresses: Vec<String>,
    pub configured_solana_addresses: Vec<String>,
}

impl Config {
    pub fn load() -> Result<Self, String> {
        let dotenv_path = load_dotenv();
        let config_directory = dotenv_path.as_deref().and_then(Path::parent);
        let (net_worth_history_path, net_worth_history) =
            net_worth_history("NET_WORTH_HISTORY_FILE", config_directory)?;

        Ok(Self {
            trading212_api_key: optional("TRADING212_API_KEY"),
            trading212_api_secret: optional("TRADING212_API_SECRET"),
            trading212_base_url: required("TRADING212_BASE_URL")?,
            coingecko_base_url: required("COINGECKO_BASE_URL")?,
            blockstream_base_url: required("BLOCKSTREAM_BASE_URL")?,
            ethereum_rpc_url: required("ETHEREUM_RPC_URL")?,
            ethereum_tokens: ethereum_tokens("ETHEREUM_ERC20_TOKENS")?,
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
            database_backup_count: whole_number("DATABASE_BACKUP_COUNT")?.clamp(1, 30),
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
            opessocius_monthly_return_rate: unit_rate("OPESSOCIUS_MONTHLY_RETURN_RATE")?,
            opessocius_return_start_month: month_start("OPESSOCIUS_RETURN_START_MONTH")?,
            opessocius_history: opessocius_history("OPESSOCIUS_HISTORY_FILE", config_directory)?,
            net_worth_history,
            net_worth_history_path,
            configured_bitcoin_xpubs: configured_list("HWR_BITCOIN_XPUBS"),
            configured_ethereum_addresses: configured_list("HWR_ETHEREUM_ADDRESSES"),
            configured_solana_addresses: configured_list("HWR_SOLANA_ADDRESSES"),
        })
    }

    pub fn trading212_is_configured(&self) -> bool {
        self.trading212_api_key.is_some() && self.trading212_api_secret.is_some()
    }
}

fn load_dotenv() -> Option<PathBuf> {
    if let Some(path) = env::var_os("SABLE_ENV_FILE").map(PathBuf::from)
        && dotenvy::from_path(&path).is_ok()
    {
        let _ = harden_private_file(&path);
        return Some(path);
    }
    if let Ok(path) = dotenvy::dotenv() {
        let _ = harden_private_file(&path);
        return Some(path);
    }
    let Ok(executable) = env::current_exe() else {
        return None;
    };
    if let Some(path) = executable
        .ancestors()
        .skip(1)
        .map(|directory| directory.join(".env"))
        .find(|candidate| candidate.is_file())
        && dotenvy::from_path(&path).is_ok()
    {
        let _ = harden_private_file(&path);
        return Some(path);
    }
    None
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

fn unit_rate(key: &str) -> Result<f64, String> {
    let rate = required(key)?
        .parse::<f64>()
        .map_err(|_| format!("{key} must be a decimal rate"))?;
    if rate.is_finite() && (0.0..=1.0).contains(&rate) {
        Ok(rate)
    } else {
        Err(format!("{key} must be between 0 and 1"))
    }
}

fn month_start(key: &str) -> Result<String, String> {
    let value = required(key)?;
    let date = NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .map_err(|_| format!("{key} must use YYYY-MM-DD format"))?;
    if date.day() != 1 {
        return Err(format!("{key} must be the first day of a month"));
    }
    Ok(date.format("%Y-%m-%d").to_string())
}

fn opessocius_history(
    key: &str,
    config_directory: Option<&Path>,
) -> Result<Vec<OpessociusHistoryRow>, String> {
    let Some(path) = optional(key) else {
        return Ok(Vec::new());
    };
    let resolved_path = resolve_local_path(&path, config_directory);
    harden_private_file(&resolved_path)?;
    let contents = fs::read_to_string(&resolved_path).map_err(|error| {
        format!(
            "Could not read {key} at {}: {error}",
            resolved_path.display()
        )
    })?;
    parse_opessocius_history(key, &contents)
}

fn net_worth_history(
    key: &str,
    config_directory: Option<&Path>,
) -> Result<(Option<PathBuf>, Vec<SaveNetWorthInput>), String> {
    let Some(path) = optional(key) else {
        return Ok((None, Vec::new()));
    };
    let resolved_path = resolve_local_path(&path, config_directory);
    harden_private_file(&resolved_path)?;
    let contents = fs::read_to_string(&resolved_path).map_err(|error| {
        format!(
            "Could not read {key} at {}: {error}",
            resolved_path.display()
        )
    })?;
    Ok((
        Some(resolved_path),
        parse_net_worth_history(key, &contents)?,
    ))
}

pub fn write_net_worth_history(path: &Path, entries: &[NetWorthEntry]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not prepare private net worth history: {error}"))?;
    }
    let mut contents = String::from(
        "date,net_worth,stocks,opessocius,crypto,savings,spending,receivables,cash,misc\n",
    );
    for entry in entries {
        writeln!(
            contents,
            "{},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2},{:.2}",
            entry.date,
            entry.net_worth,
            entry.stocks,
            entry.opessocius,
            entry.crypto,
            entry.savings,
            entry.spending,
            entry.receivables,
            entry.cash,
            entry.misc,
        )
        .map_err(|error| format!("Could not format private net worth history: {error}"))?;
    }
    let temporary_path = path.with_extension("csv.tmp");
    fs::write(&temporary_path, contents)
        .map_err(|error| format!("Could not update private net worth history: {error}"))?;
    harden_private_file(&temporary_path)?;
    fs::rename(&temporary_path, path)
        .map_err(|error| format!("Could not replace private net worth history: {error}"))?;
    harden_private_file(path)
}

#[cfg(unix)]
pub(crate) fn harden_private_file(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)
        .map_err(|error| format!("Could not inspect private file {}: {error}", path.display()))?;
    let mut permissions = metadata.permissions();
    if permissions.mode() & 0o077 != 0 {
        permissions.set_mode(0o600);
        fs::set_permissions(path, permissions).map_err(|error| {
            format!(
                "Could not restrict private file {} to its owner: {error}",
                path.display()
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn harden_private_file(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn parse_net_worth_history(key: &str, contents: &str) -> Result<Vec<SaveNetWorthInput>, String> {
    let mut entries = Vec::new();
    for (index, line) in contents.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 10 {
            return Err(format!("{key} row {} must contain 10 columns", index + 1));
        }
        let date = NaiveDate::parse_from_str(fields[0], "%Y-%m-%d")
            .map_err(|_| format!("{key} row {} has an invalid date", index + 1))?
            .format("%Y-%m-%d")
            .to_string();
        let declared_total = non_negative_csv_value(key, "net_worth", fields[1], index + 1)?;
        let entry = SaveNetWorthInput {
            date,
            stocks: non_negative_csv_value(key, "stocks", fields[2], index + 1)?,
            opessocius: non_negative_csv_value(key, "opessocius", fields[3], index + 1)?,
            crypto: non_negative_csv_value(key, "crypto", fields[4], index + 1)?,
            savings: non_negative_csv_value(key, "savings", fields[5], index + 1)?,
            spending: non_negative_csv_value(key, "spending", fields[6], index + 1)?,
            receivables: non_negative_csv_value(key, "receivables", fields[7], index + 1)?,
            cash: non_negative_csv_value(key, "cash", fields[8], index + 1)?,
            misc: non_negative_csv_value(key, "misc", fields[9], index + 1)?,
        };
        if (entry.net_worth() - declared_total).abs() > 0.02 {
            return Err(format!(
                "{key} row {} categories do not equal net_worth",
                index + 1
            ));
        }
        entries.push(entry);
    }
    entries.sort_by(|left, right| left.date.cmp(&right.date));
    if entries.windows(2).any(|pair| pair[0].date == pair[1].date) {
        return Err(format!("{key} contains duplicate dates"));
    }
    Ok(entries)
}

fn non_negative_csv_value(key: &str, column: &str, value: &str, row: usize) -> Result<f64, String> {
    let amount = decimal_value(key, column, value, row)?;
    if amount >= 0.0 {
        Ok(amount)
    } else {
        Err(format!("{key} row {row} has a negative {column}"))
    }
}

fn resolve_local_path(configured_path: &str, config_directory: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(configured_path);
    if path.is_absolute() {
        path
    } else if let Some(directory) = config_directory {
        directory.join(path)
    } else {
        path
    }
}

fn parse_opessocius_history(
    key: &str,
    contents: &str,
) -> Result<Vec<OpessociusHistoryRow>, String> {
    let mut rows = Vec::new();
    for (index, line) in contents.lines().enumerate().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 6 {
            return Err(format!("{key} row {} must contain 6 columns", index + 1));
        }
        let month = month_value(key, fields[0], index + 1)?;
        rows.push(OpessociusHistoryRow {
            month,
            return_percent: decimal_value(key, "return_percent", fields[1], index + 1)?,
            return_eur: decimal_value(key, "return_eur", fields[2], index + 1)?,
            deposits_eur: decimal_value(key, "deposits_eur", fields[3], index + 1)?,
            withdrawals_eur: decimal_value(key, "withdrawals_eur", fields[4], index + 1)?,
            ending_balance_eur: decimal_value(key, "ending_balance_eur", fields[5], index + 1)?,
        });
    }
    rows.sort_by(|left, right| left.month.cmp(&right.month));
    Ok(rows)
}

fn month_value(key: &str, value: &str, row: usize) -> Result<String, String> {
    let date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| format!("{key} row {row} has an invalid month"))?;
    if date.day() != 1 {
        return Err(format!("{key} row {row} must start on the first day"));
    }
    Ok(date.format("%Y-%m-%d").to_string())
}

fn decimal_value(key: &str, column: &str, value: &str, row: usize) -> Result<f64, String> {
    let number = value
        .parse::<f64>()
        .map_err(|_| format!("{key} row {row} has an invalid {column}"))?;
    if number.is_finite() {
        Ok(number)
    } else {
        Err(format!("{key} row {row} has an invalid {column}"))
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

fn ethereum_tokens(key: &str) -> Result<Vec<EthereumTokenConfig>, String> {
    let Some(value) = optional(key) else {
        return Ok(Vec::new());
    };
    value
        .split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .enumerate()
        .map(|(index, entry)| {
            let fields = entry.split('|').map(str::trim).collect::<Vec<_>>();
            if fields.len() != 5 || fields.iter().any(|field| field.is_empty()) {
                return Err(format!(
                    "{key} entry {} must contain price-id|symbol|name|contract|decimals",
                    index + 1
                ));
            }
            let contract_address = fields[3];
            if contract_address.len() != 42
                || !contract_address.starts_with("0x")
                || !contract_address[2..]
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
            {
                return Err(format!("{key} entry {} has an invalid contract", index + 1));
            }
            let decimals = fields[4]
                .parse::<u32>()
                .map_err(|_| format!("{key} entry {} has invalid decimals", index + 1))?;
            if decimals > 38 {
                return Err(format!(
                    "{key} entry {} decimals must be at most 38",
                    index + 1
                ));
            }
            Ok(EthereumTokenConfig {
                price_id: fields[0].to_lowercase(),
                symbol: fields[1].to_uppercase(),
                name: fields[2].to_string(),
                contract_address: contract_address.to_string(),
                decimals,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        ethereum_tokens, parse_net_worth_history, parse_opessocius_history, resolve_local_path,
    };

    #[test]
    fn parses_private_net_worth_history_and_validates_the_total() {
        let entries = parse_net_worth_history(
            "NET_WORTH_HISTORY_FILE",
            "date,net_worth,stocks,opessocius,crypto,savings,spending,receivables,cash,misc\n\
             2026-07-27,20364.31,10919.67,8028.04,0,125,217.85,1033.75,40,0\n",
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].net_worth() - 20_364.31).abs() < 0.001);
        assert_eq!(entries[0].receivables, 1_033.75);
    }

    #[test]
    fn parses_configured_ethereum_tokens() {
        unsafe {
            std::env::set_var(
                "TEST_ETHEREUM_TOKENS",
                "chainlink|LINK|Chainlink|0x514910771AF9Ca656af840dff83E8264EcF986CA|18",
            );
        }
        let tokens = ethereum_tokens("TEST_ETHEREUM_TOKENS").unwrap();
        unsafe { std::env::remove_var("TEST_ETHEREUM_TOKENS") };
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].price_id, "chainlink");
        assert_eq!(tokens[0].symbol, "LINK");
        assert_eq!(tokens[0].decimals, 18);
    }

    #[test]
    fn parses_and_orders_private_opessocius_history() {
        let rows = parse_opessocius_history(
            "OPESSOCIUS_HISTORY_FILE",
            "month,return_percent,return_eur,deposits_eur,withdrawals_eur,ending_balance_eur\n\
             2026-07-01,2,157.41,0,0,8028.05\n\
             2026-06-01,2,150.35,200,0,7870.63\n",
        )
        .unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].month, "2026-06-01");
        assert_eq!(rows[1].ending_balance_eur, 8028.05);
        assert_eq!(rows[0].deposits_eur, 200.0);
    }

    #[test]
    fn resolves_private_data_relative_to_the_dotenv_file() {
        assert_eq!(
            resolve_local_path(
                "data/opessocius-history.csv",
                Some(Path::new("/projects/sable")),
            ),
            Path::new("/projects/sable/data/opessocius-history.csv")
        );
        assert_eq!(
            resolve_local_path("/private/history.csv", Some(Path::new("/projects/sable"))),
            Path::new("/private/history.csv")
        );
    }
}
