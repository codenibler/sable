use std::collections::{HashMap, HashSet};

use reqwest::Client;
use serde_json::{Value, json};

use chrono::{DateTime, Duration, Utc};

use crate::{
    config::Config,
    models::{Wallet, WalletAsset},
    providers::bitcoin_xpub,
};

const EVERSTAKE_STAKE_ADDED_TOPIC: &str =
    "0x7d194e8dc0f902cdc51bde00649039561dbd0b01574d671bad333436fdac7692";
const EVERSTAKE_UNSTAKE_TOPIC: &str =
    "0x0750a71dce555de583ab0225a108df42b9402d22123d7cc9cd95793e43e7db0e";
const EVERSTAKE_STAKE_CANCELED_TOPIC: &str =
    "0x54e9536cd034f3df0a8d955ac91a16ad1658352edd5d61d9d7c98616a80ad73f";

pub async fn prices(client: &Client, config: &Config) -> Result<AssetPrices, String> {
    let mut ids = vec![
        "bitcoin".to_string(),
        "ethereum".to_string(),
        "solana".to_string(),
    ];
    ids.extend(
        config
            .ethereum_tokens
            .iter()
            .map(|token| token.price_id.clone()),
    );
    ids.sort();
    ids.dedup();
    let ids = ids.join(",");
    let currency = config.base_currency.to_lowercase();
    let response: Value = client
        .get(format!(
            "{}/simple/price",
            config.coingecko_base_url.trim_end_matches('/')
        ))
        .query(&[("ids", ids.as_str()), ("vs_currencies", currency.as_str())])
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

    let values = ids
        .split(',')
        .map(|id| Ok((id.to_string(), price(&response, id, &currency)?)))
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(AssetPrices { values })
}

pub async fn hydrate_wallet(
    client: &Client,
    config: &Config,
    wallet: &mut Wallet,
    prices: &AssetPrices,
) -> bool {
    if wallet.network == "btc" && wallet.wallet_type == "xpub" {
        let price = prices.get("bitcoin");
        if cache_is_fresh(wallet, config.xpub_refresh_interval_minutes) {
            wallet.symbol = "BTC".to_string();
            wallet.value = wallet.balance * price;
            wallet.assets = vec![wallet_asset(
                "btc",
                "btc",
                "BTC",
                "Bitcoin",
                wallet.balance,
                price,
                None,
            )];
            wallet.message = None;
            wallet.error = None;
            return false;
        }
        match bitcoin_xpub::scan(client, config, &wallet.address).await {
            Ok(result) => {
                wallet.balance = result.balance;
                wallet.address_count = result.address_count;
                wallet.symbol = "BTC".to_string();
                wallet.value = result.balance * price;
                wallet.assets = vec![wallet_asset(
                    "btc",
                    "btc",
                    "BTC",
                    "Bitcoin",
                    result.balance,
                    price,
                    None,
                )];
                wallet.message = None;
                wallet.error = None;
                return true;
            }
            Err(message) => {
                wallet.value = wallet.balance * price;
                wallet.assets = vec![wallet_asset(
                    "btc",
                    "btc",
                    "BTC",
                    "Bitcoin",
                    wallet.balance,
                    price,
                    Some(message.clone()),
                )];
                wallet.message = Some(message.clone());
                wallet.error = Some(message);
                return false;
            }
        }
    }
    if wallet.network == "eth" && wallet.wallet_type == "everstake" {
        hydrate_everstake_wallet(client, config, wallet, prices).await;
        return false;
    }
    if wallet.network == "eth" {
        hydrate_ethereum_wallet(client, config, wallet, prices).await;
        return false;
    }
    let result = match wallet.network.as_str() {
        "btc" => btc_balance(client, config, &wallet.address)
            .await
            .map(|balance| (balance, prices.get("bitcoin"), "btc", "BTC", "Bitcoin")),
        "sol" => sol_balance(client, config, &wallet.address)
            .await
            .map(|balance| (balance, prices.get("solana"), "sol", "SOL", "Solana")),
        _ => Err("Unsupported wallet network".to_string()),
    };
    match result {
        Ok((balance, price, id, symbol, name)) => {
            wallet.balance = balance;
            wallet.symbol = symbol.to_string();
            wallet.value = balance * price;
            wallet.assets = vec![wallet_asset(
                id,
                &wallet.network,
                symbol,
                name,
                balance,
                price,
                None,
            )];
            wallet.message = None;
            wallet.error = None;
        }
        Err(message) => {
            wallet.message = Some(message.clone());
            wallet.error = Some(message);
        }
    }
    false
}

async fn hydrate_everstake_wallet(
    client: &Client,
    config: &Config,
    wallet: &mut Wallet,
    prices: &AssetPrices,
) {
    match everstake_balance(client, config, &wallet.address).await {
        Ok(balance) => {
            let price = prices.get("ethereum");
            wallet.balance = balance;
            wallet.symbol = "ETH".to_string();
            wallet.value = balance * price;
            wallet.assets = vec![wallet_asset(
                "eth",
                "eth",
                "ETH",
                "Ethereum",
                balance,
                price,
                Some("Everstake pooled staking".to_string()),
            )];
            wallet.message = Some("Everstake pooled staking".to_string());
            wallet.error = None;
        }
        Err(message) => {
            wallet.balance = 0.0;
            wallet.value = 0.0;
            wallet.assets.clear();
            wallet.message = Some(message.clone());
            wallet.error = Some(message);
        }
    }
}

async fn hydrate_ethereum_wallet(
    client: &Client,
    config: &Config,
    wallet: &mut Wallet,
    prices: &AssetPrices,
) {
    let mut assets = Vec::new();
    let mut messages = Vec::new();
    match eth_balance(client, config, &wallet.address).await {
        Ok(balance) => {
            let price = prices.get("ethereum");
            wallet.balance = balance;
            assets.push(wallet_asset(
                "eth", "eth", "ETH", "Ethereum", balance, price, None,
            ));
        }
        Err(message) => {
            wallet.balance = 0.0;
            messages.push(message);
        }
    }
    for token in &config.ethereum_tokens {
        match erc20_balance(
            client,
            config,
            &wallet.address,
            &token.contract_address,
            token.decimals,
        )
        .await
        {
            Ok(balance) if balance > 0.0 => assets.push(wallet_asset(
                &token.symbol.to_lowercase(),
                "eth",
                &token.symbol,
                &token.name,
                balance,
                prices.get(&token.price_id),
                None,
            )),
            Ok(_) => {}
            Err(message) => messages.push(format!("{}: {message}", token.symbol)),
        }
    }
    wallet.symbol = "ETH".to_string();
    wallet.value = assets.iter().map(|asset| asset.value).sum();
    wallet.assets = assets;
    wallet.message = (!messages.is_empty()).then(|| messages.join(" · "));
    wallet.error = wallet.message.clone();
}

fn wallet_asset(
    id: &str,
    network: &str,
    symbol: &str,
    name: &str,
    balance: f64,
    price: f64,
    message: Option<String>,
) -> WalletAsset {
    WalletAsset {
        id: id.to_string(),
        network: network.to_string(),
        symbol: symbol.to_string(),
        name: name.to_string(),
        balance,
        price,
        value: balance * price,
        message,
    }
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
    values: HashMap<String, f64>,
}

impl AssetPrices {
    fn get(&self, id: &str) -> f64 {
        self.values.get(id).copied().unwrap_or_default()
    }
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

async fn erc20_balance(
    client: &Client,
    config: &Config,
    owner: &str,
    contract: &str,
    decimals: u32,
) -> Result<f64, String> {
    let response: Value = client
        .post(&config.ethereum_rpc_url)
        .json(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "eth_call",
            "params": [{ "to": contract, "data": balance_of_data(owner) }, "latest"]
        }))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(provider_status)?
        .json()
        .await
        .map_err(|_| "Ethereum provider returned an unreadable token response".to_string())?;
    let value = response
        .pointer("/result")
        .and_then(Value::as_str)
        .ok_or_else(|| "Ethereum provider did not return a token balance".to_string())?
        .trim_start_matches("0x");
    if value.is_empty() {
        return Ok(0.0);
    }
    let units = u128::from_str_radix(value, 16)
        .map_err(|_| "Ethereum token balance was invalid".to_string())?;
    Ok(units as f64 / 10_f64.powi(decimals as i32))
}

async fn everstake_balance(
    client: &Client,
    config: &Config,
    pool_address: &str,
) -> Result<f64, String> {
    if config.configured_ethereum_addresses.is_empty() {
        return Err("Everstake tracking requires HWR_ETHEREUM_ADDRESSES".to_string());
    }
    let url = format!(
        "{}/addresses/{}/logs",
        config.ethereum_explorer_api_url.trim_end_matches('/'),
        pool_address.trim()
    );
    let mut deposits = 0_u128;
    let mut withdrawals = 0_u128;
    let mut owners = HashSet::new();
    for owner in &config.configured_ethereum_addresses {
        if !owners.insert(owner.trim().to_lowercase()) {
            continue;
        }
        let owner_topic = format!(
            "0x{:0>64}",
            owner.trim().trim_start_matches("0x").to_lowercase()
        );
        let mut next_page = None;
        for page in 0..100 {
            let mut query = vec![("topic".to_string(), owner_topic.clone())];
            if let Some(Value::Object(parameters)) = &next_page {
                query.extend(parameters.iter().filter_map(|(key, value)| {
                    let value = value
                        .as_str()
                        .map(str::to_string)
                        .or_else(|| value.as_u64().map(|number| number.to_string()))
                        .or_else(|| value.as_i64().map(|number| number.to_string()))?;
                    Some((key.clone(), value))
                }));
            }
            let response: Value = client
                .get(&url)
                .query(&query)
                .send()
                .await
                .map_err(network_error)?
                .error_for_status()
                .map_err(|error| {
                    format!(
                        "Ethereum explorer returned HTTP {}",
                        error
                            .status()
                            .map_or("unknown".into(), |status| status.to_string())
                    )
                })?
                .json()
                .await
                .map_err(|_| "Ethereum explorer returned an unreadable response".to_string())?;
            let logs = response
                .pointer("/items")
                .and_then(Value::as_array)
                .ok_or_else(|| "Ethereum explorer did not return Everstake events".to_string())?;
            for log in logs {
                if let Some((is_deposit, value)) = everstake_log_value(log)? {
                    if is_deposit {
                        deposits = deposits
                            .checked_add(value)
                            .ok_or_else(|| "Everstake balance overflowed".to_string())?;
                    } else {
                        withdrawals = withdrawals
                            .checked_add(value)
                            .ok_or_else(|| "Everstake balance overflowed".to_string())?;
                    }
                }
            }
            next_page = response.get("next_page_params").cloned();
            if next_page.as_ref().is_none_or(Value::is_null) {
                break;
            }
            if page == 99 {
                return Err("Everstake event history exceeded the supported page limit".to_string());
            }
        }
    }
    let balance = deposits
        .checked_sub(withdrawals)
        .ok_or_else(|| "Everstake withdrawals exceed recorded deposits".to_string())?;
    Ok(balance as f64 / 1_000_000_000_000_000_000.0)
}

fn everstake_log_value(log: &Value) -> Result<Option<(bool, u128)>, String> {
    let topic = log
        .pointer("/topics/0")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase();
    let is_deposit = topic == EVERSTAKE_STAKE_ADDED_TOPIC;
    if !is_deposit && topic != EVERSTAKE_UNSTAKE_TOPIC && topic != EVERSTAKE_STAKE_CANCELED_TOPIC {
        return Ok(None);
    }
    let data = log
        .get("data")
        .and_then(Value::as_str)
        .and_then(|data| data.strip_prefix("0x"))
        .ok_or_else(|| "Everstake event contained invalid data".to_string())?;
    let value = data
        .get(..64)
        .ok_or_else(|| "Everstake event contained invalid data".to_string())?;
    let value = u128::from_str_radix(value, 16)
        .map_err(|_| "Everstake event contained an invalid amount".to_string())?;
    Ok(Some((is_deposit, value)))
}

fn balance_of_data(owner: &str) -> String {
    format!(
        "0x70a08231{:0>64}",
        owner.trim().trim_start_matches("0x").to_lowercase()
    )
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
    use serde_json::json;

    use super::{
        EVERSTAKE_STAKE_ADDED_TOPIC, EVERSTAKE_UNSTAKE_TOPIC, balance_of_data, everstake_log_value,
        validate_wallet,
    };

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

    #[test]
    fn encodes_erc20_balance_queries_for_an_ethereum_owner() {
        let data = balance_of_data("0x00000000000000000000000000000000000000ab");
        assert_eq!(data.len(), 74);
        assert_eq!(
            data,
            "0x70a0823100000000000000000000000000000000000000000000000000000000000000ab"
        );
    }

    #[test]
    fn decodes_everstake_position_events() {
        let deposit = json!({
            "topics": [EVERSTAKE_STAKE_ADDED_TOPIC],
            "data": "0x00000000000000000000000000000000000000000000000001717b72f0a400000000000000000000000000000000000000000000000000000000000000000001"
        });
        let withdrawal = json!({
            "topics": [EVERSTAKE_UNSTAKE_TOPIC],
            "data": "0x000000000000000000000000000000000000000000000000002386f26fc10000"
        });
        assert_eq!(
            everstake_log_value(&deposit).unwrap(),
            Some((true, 104_000_000_000_000_000))
        );
        assert_eq!(
            everstake_log_value(&withdrawal).unwrap(),
            Some((false, 10_000_000_000_000_000))
        );
    }
}
