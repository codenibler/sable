use bitcoin::{
    Address, CompressedPublicKey, Network, base58,
    bip32::{ChildNumber, Xpub},
    secp256k1::Secp256k1,
};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde_json::Value;

use crate::config::Config;

const XPUB_VERSION: u32 = 0x0488_b21e;
const YPUB_VERSION: u32 = 0x049d_7cb2;
const ZPUB_VERSION: u32 = 0x04b2_4746;

#[derive(Clone, Copy)]
enum AddressFormat {
    Legacy,
    NestedSegwit,
    NativeSegwit,
}

pub struct ExtendedPublicKey {
    key: Xpub,
    format: AddressFormat,
}

impl ExtendedPublicKey {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut payload = base58::decode_check(value.trim())
            .map_err(|_| "Bitcoin XPUB is not valid Base58Check data".to_string())?;
        if payload.len() != 78 {
            return Err("Bitcoin XPUB has an invalid length".to_string());
        }

        let version = u32::from_be_bytes(
            payload[0..4]
                .try_into()
                .map_err(|_| "Bitcoin XPUB has an invalid version".to_string())?,
        );
        let format = match version {
            XPUB_VERSION => AddressFormat::Legacy,
            YPUB_VERSION => AddressFormat::NestedSegwit,
            ZPUB_VERSION => AddressFormat::NativeSegwit,
            _ => return Err("Only mainnet xpub, ypub, and zpub keys are supported".to_string()),
        };
        payload[0..4].copy_from_slice(&XPUB_VERSION.to_be_bytes());
        let key = Xpub::decode(&payload).map_err(|_| "Bitcoin XPUB data is invalid".to_string())?;
        if key.depth != 3 {
            return Err("Use an account-level Bitcoin XPUB (derivation depth 3)".to_string());
        }
        Ok(Self { key, format })
    }

    pub fn derive_address(&self, change: u32, index: u32) -> Result<String, String> {
        let path = [
            ChildNumber::from_normal_idx(change)
                .map_err(|_| "Bitcoin change branch is invalid".to_string())?,
            ChildNumber::from_normal_idx(index)
                .map_err(|_| "Bitcoin address index is invalid".to_string())?,
        ];
        let derived = self
            .key
            .derive_pub(&Secp256k1::verification_only(), &path)
            .map_err(|_| "Could not derive a Bitcoin XPUB address".to_string())?;
        let compressed = CompressedPublicKey(derived.public_key);
        let address = match self.format {
            AddressFormat::Legacy => Address::p2pkh(compressed, Network::Bitcoin),
            AddressFormat::NestedSegwit => Address::p2shwpkh(&compressed, Network::Bitcoin),
            AddressFormat::NativeSegwit => Address::p2wpkh(&compressed, Network::Bitcoin),
        };
        Ok(address.to_string())
    }
}

pub fn validate(value: &str) -> Result<(), String> {
    ExtendedPublicKey::parse(value).map(|_| ())
}

pub struct ScanResult {
    pub balance: f64,
    pub address_count: usize,
}

pub async fn scan(client: &Client, config: &Config, value: &str) -> Result<ScanResult, String> {
    let key = ExtendedPublicKey::parse(value)?;
    let mut balance_sats = 0_i64;
    let mut address_count = 0_usize;
    for branch in [0_u32, 1_u32] {
        let branch_result = scan_branch(client, config, &key, branch).await?;
        balance_sats += branch_result.0;
        address_count += branch_result.1;
    }
    Ok(ScanResult {
        balance: balance_sats as f64 / 100_000_000.0,
        address_count,
    })
}

async fn scan_branch(
    client: &Client,
    config: &Config,
    key: &ExtendedPublicKey,
    branch: u32,
) -> Result<(i64, usize), String> {
    let limit = config
        .xpub_max_addresses_per_branch
        .max(config.xpub_gap_limit);
    let mut next_index = 0_usize;
    let mut unused_run = 0_usize;
    let mut balance_sats = 0_i64;
    let mut address_count = 0_usize;

    while next_index < limit && unused_run < config.xpub_gap_limit {
        let end = (next_index + config.xpub_scan_concurrency).min(limit);
        let addresses = (next_index..end)
            .map(|index| {
                key.derive_address(branch, index as u32)
                    .map(|address| (index, address))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut results = stream::iter(addresses)
            .map(|(index, address)| async move {
                address_activity(client, config, &address)
                    .await
                    .map(|activity| (index, activity))
            })
            .buffer_unordered(config.xpub_scan_concurrency)
            .collect::<Vec<_>>()
            .await;
        results.sort_by_key(|result| {
            result
                .as_ref()
                .map(|(index, _)| *index)
                .unwrap_or(usize::MAX)
        });

        for result in results {
            let (_, (balance, used)) = result?;
            balance_sats += balance;
            if used {
                address_count += 1;
                unused_run = 0;
            } else {
                unused_run += 1;
            }
            if unused_run >= config.xpub_gap_limit {
                break;
            }
        }
        next_index = end;
    }

    if next_index >= limit && unused_run < config.xpub_gap_limit {
        return Err(
            "Bitcoin XPUB scan reached XPUB_MAX_ADDRESSES_PER_BRANCH before finding the gap limit"
                .to_string(),
        );
    }
    Ok((balance_sats, address_count))
}

async fn address_activity(
    client: &Client,
    config: &Config,
    address: &str,
) -> Result<(i64, bool), String> {
    let response: Value = client
        .get(format!(
            "{}/address/{address}",
            config.blockstream_base_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                "Bitcoin XPUB provider timed out".to_string()
            } else {
                "Could not reach the Bitcoin XPUB provider".to_string()
            }
        })?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Bitcoin XPUB provider returned HTTP {}",
                error
                    .status()
                    .map_or("unknown".into(), |status| status.to_string())
            )
        })?
        .json()
        .await
        .map_err(|_| "Bitcoin XPUB provider returned an unreadable response".to_string())?;
    let confirmed = integer(&response, "/chain_stats/funded_txo_sum")
        - integer(&response, "/chain_stats/spent_txo_sum");
    let pending = integer(&response, "/mempool_stats/funded_txo_sum")
        - integer(&response, "/mempool_stats/spent_txo_sum");
    let transaction_count =
        integer(&response, "/chain_stats/tx_count") + integer(&response, "/mempool_stats/tx_count");
    Ok((confirmed + pending, transaction_count > 0))
}

fn integer(value: &Value, pointer: &str) -> i64 {
    value
        .pointer(pointer)
        .and_then(Value::as_i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{ExtendedPublicKey, validate};

    const BIP84_ACCOUNT: &str = "zpub6rFR7y4Q2AijBEqTUquhVz398htDFrtymD9xYYfG1m4wAcvPhXNfE3EfH1r1ADqtfSdVCToUG868RvUUkgDKf31mGDtKsAYz2oz2AGutZYs";

    #[test]
    fn derives_official_bip84_receive_and_change_addresses() {
        let key = ExtendedPublicKey::parse(BIP84_ACCOUNT).expect("BIP84 account key");
        assert_eq!(
            key.derive_address(0, 0).unwrap(),
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        );
        assert_eq!(
            key.derive_address(0, 1).unwrap(),
            "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"
        );
        assert_eq!(
            key.derive_address(1, 0).unwrap(),
            "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"
        );
    }

    #[test]
    fn rejects_invalid_or_non_account_extended_keys() {
        assert!(validate("not-an-xpub").is_err());
    }
}
