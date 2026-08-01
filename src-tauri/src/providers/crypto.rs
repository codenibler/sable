use reqwest::Client;
use serde_json::{Value, json};

use chrono::{DateTime, Duration, Utc};

use crate::{config::Config, models::Wallet, providers::bitcoin_xpub};

pub async fn prices(client: &Client, config: &Config) -> Result<AssetPrices, String> {
    let response: Value = client
        .get(format!(
            "{}/simple/price",
            config.coingecko_base_url.trim_end_matches('/')
        ))
        .query(&[
            ("ids", "bitcoin,ethereum,solana"),
            (
                "vs_currencies",
                config.base_currency.to_lowercase().as_str(),
            ),
        ])
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Price provider returned HTTP {}",
                error.status().map_or("unknown".into(), |s| s.to_string())
            )
        })?
        .json()
        .await
        .map_err(|_| "Price provider returned an unreadable response".to_string())?;

    let currency = config.base_currency.to_lowercase();
    Ok(AssetPrices {
        btc: price(&response, "bitcoin", &currency)?,
        eth: price(&response, "ethereum", &currency)?,
        sol: price(&response, "solana", &currency)?,
    })
}

pub async fn hydrate_wallet(
    client: &Client,
    config: &Config,
    wallet: &mut Wallet,
    prices: &AssetPrices,
) -> bool {
    if wallet.network == "btc" && wallet.wallet_type == "xpub" {
        if cache_is_fresh(wallet, config.xpub_refresh_interval_minutes) {
            wallet.symbol = "BTC".to_string();
            wallet.value = wallet.balance * prices.btc;
            wallet.message = None;
            return false;
        }
        match bitcoin_xpub::scan(client, config, &wallet.address).await {
            Ok(result) => {
                wallet.balance = result.balance;
                wallet.address_count = result.address_count;
                wallet.symbol = "BTC".to_string();
                wallet.value = result.balance * prices.btc;
                wallet.message = None;
                return true;
            }
            Err(message) => {
                wallet.value = wallet.balance * prices.btc;
                wallet.message = Some(message);
                return false;
            }
        }
    }
    let result = match wallet.network.as_str() {
        "btc" => btc_balance(client, config, &wallet.address)
            .await
            .map(|balance| (balance, prices.btc, "BTC")),
        "eth" => eth_balance(client, config, &wallet.address)
            .await
            .map(|balance| (balance, prices.eth, "ETH")),
        "sol" => sol_balance(client, config, &wallet.address)
            .await
            .map(|balance| (balance, prices.sol, "SOL")),
        _ => Err("Unsupported wallet network".to_string()),
    };
    match result {
        Ok((balance, price, symbol)) => {
            wallet.balance = balance;
            wallet.symbol = symbol.to_string();
            wallet.value = balance * price;
            wallet.message = None;
        }
        Err(message) => wallet.message = Some(message),
    }
    false
}

pub struct ValidatedWallet {
    pub network: String,
    pub wallet_type: String,
}

pub fn validate_wallet(network: &str, address: &str) -> Result<ValidatedWallet, String> {
    let normalized_network = network.trim().to_lowercase();
    let address = address.trim();
    let is_extended_key = ["xpub", "ypub", "zpub"]
        .iter()
        .any(|prefix| address.starts_with(prefix));
    if normalized_network == "btc-xpub" || (normalized_network == "btc" && is_extended_key) {
        bitcoin_xpub::validate(address)?;
        return Ok(ValidatedWallet {
            network: "btc".to_string(),
            wallet_type: "xpub".to_string(),
        });
    }
    let valid = match normalized_network.as_str() {
        "btc" => {
            (address.starts_with('1')
                || address.starts_with('3')
                || address.to_lowercase().starts_with("bc1"))
                && (26..=90).contains(&address.len())
        }
        "eth" => {
            address.len() == 42
                && address.starts_with("0x")
                && address[2..]
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        }
        "sol" => {
            (32..=44).contains(&address.len())
                && address.chars().all(|character| {
                    "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(character)
                })
        }
        _ => return Err("Network must be BTC, BTC XPUB, ETH, or SOL".to_string()),
    };
    if valid {
        Ok(ValidatedWallet {
            network: normalized_network,
            wallet_type: "address".to_string(),
        })
    } else {
        Err(format!(
            "That does not look like a valid {} address",
            normalized_network.to_uppercase()
        ))
    }
}

fn cache_is_fresh(wallet: &Wallet, refresh_interval_minutes: i64) -> bool {
    wallet
        .last_checked_at
        .as_deref()
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())
        .is_some_and(|checked| {
            Utc::now() - checked < Duration::minutes(refresh_interval_minutes.max(0))
        })
}

pub struct AssetPrices {
    btc: f64,
    eth: f64,
    sol: f64,
}

fn price(response: &Value, asset: &str, currency: &str) -> Result<f64, String> {
    response
        .pointer(&format!("/{asset}/{currency}"))
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("No {asset} price was returned"))
}

async fn btc_balance(client: &Client, config: &Config, address: &str) -> Result<f64, String> {
    let response: Value = client
        .get(format!(
            "{}/address/{address}",
            config.blockstream_base_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(provider_status)?
        .json()
        .await
        .map_err(|_| "Bitcoin provider returned an unreadable response".to_string())?;
    let confirmed = integer(&response, "/chain_stats/funded_txo_sum")
        - integer(&response, "/chain_stats/spent_txo_sum");
    let pending = integer(&response, "/mempool_stats/funded_txo_sum")
        - integer(&response, "/mempool_stats/spent_txo_sum");
    Ok((confirmed + pending) as f64 / 100_000_000.0)
}

async fn eth_balance(client: &Client, config: &Config, address: &str) -> Result<f64, String> {
    let response: Value = client
        .post(&config.ethereum_rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_getBalance",
            "params": [address, "latest"]
        }))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(provider_status)?
        .json()
        .await
        .map_err(|_| "Ethereum provider returned an unreadable response".to_string())?;
    let value = response
        .pointer("/result")
        .and_then(Value::as_str)
        .ok_or_else(|| "Ethereum provider did not return a balance".to_string())?
        .trim_start_matches("0x");
    let wei =
        u128::from_str_radix(value, 16).map_err(|_| "Ethereum balance was invalid".to_string())?;
    Ok(wei as f64 / 1_000_000_000_000_000_000.0)
}

async fn sol_balance(client: &Client, config: &Config, address: &str) -> Result<f64, String> {
    let response: Value = client
        .post(&config.solana_rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getBalance",
            "params": [address, { "commitment": "confirmed" }]
        }))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(provider_status)?
        .json()
        .await
        .map_err(|_| "Solana provider returned an unreadable response".to_string())?;
    let lamports = response
        .pointer("/result/value")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Solana provider did not return a balance".to_string())?;
    Ok(lamports as f64 / 1_000_000_000.0)
}

fn integer(value: &Value, pointer: &str) -> i64 {
    value
        .pointer(pointer)
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "Wallet provider timed out".to_string()
    } else {
        "Could not reach the wallet provider".to_string()
    }
}

fn provider_status(error: reqwest::Error) -> String {
    format!(
        "Wallet provider returned HTTP {}",
        error.status().map_or("unknown".into(), |s| s.to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::validate_wallet;

    #[test]
    fn validates_supported_wallet_shapes() {
        assert!(validate_wallet("eth", "0x0000000000000000000000000000000000000000").is_ok());
        assert!(validate_wallet("btc", "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh").is_ok());
        assert!(validate_wallet("sol", "11111111111111111111111111111111").is_ok());
    }

    #[test]
    fn rejects_unknown_networks_and_malformed_addresses() {
        assert!(validate_wallet("doge", "anything").is_err());
        assert!(validate_wallet("eth", "0x123").is_err());
    }
}
