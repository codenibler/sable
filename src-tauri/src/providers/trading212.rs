use reqwest::Client;
use serde_json::Value;

use crate::{
    config::Config,
    models::{CashEvent, Holding, TradingOverview, TransactionPage},
};

pub async fn fetch_overview(client: &Client, config: &Config) -> Result<TradingOverview, String> {
    let key = config
        .trading212_api_key
        .as_ref()
        .ok_or_else(|| "Trading 212 API key is not configured".to_string())?;
    let secret = config
        .trading212_api_secret
        .as_ref()
        .ok_or_else(|| "Trading 212 API secret is not configured".to_string())?;

    let summary: Value = get(client, config, key, secret, "/equity/account/summary").await?;
    let positions: Value = get(client, config, key, secret, "/equity/positions").await?;
    let currency = text(&summary, "/currency").unwrap_or("EUR");
    let rate = currency_rate(client, config, currency).await?;

    let total_value = number(&summary, "/totalValue") * rate;
    let cash_value = (number(&summary, "/cash/availableToTrade")
        + number(&summary, "/cash/inPies")
        + number(&summary, "/cash/reservedForOrders"))
        * rate;
    let invested_value = number(&summary, "/investments/totalCost") * rate;
    let return_value = (number(&summary, "/investments/realizedProfitLoss")
        + number(&summary, "/investments/unrealizedProfitLoss"))
        * rate;

    let mut holdings = positions
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, position)| position_to_holding(position, index, rate))
        .collect::<Vec<_>>();
    holdings.sort_by(|left, right| right.value.total_cmp(&left.value));

    Ok(TradingOverview {
        total_value,
        invested_value,
        cash_value,
        return_value,
        holdings,
    })
}

pub async fn fetch_transaction_page(
    client: &Client,
    config: &Config,
    path: &str,
) -> Result<TransactionPage, String> {
    let key = config
        .trading212_api_key
        .as_ref()
        .ok_or_else(|| "Trading 212 API key is not configured".to_string())?;
    let secret = config
        .trading212_api_secret
        .as_ref()
        .ok_or_else(|| "Trading 212 API secret is not configured".to_string())?;
    let response = get(client, config, key, secret, path).await?;
    let events = response
        .pointer("/items")
        .and_then(Value::as_array)
        .ok_or_else(|| "Trading 212 transaction history was unreadable".to_string())?
        .iter()
        .filter_map(transaction_from_value)
        .collect();
    let next_path = response
        .pointer("/nextPagePath")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(TransactionPage { events, next_path })
}

async fn get(
    client: &Client,
    config: &Config,
    key: &str,
    secret: &str,
    path: &str,
) -> Result<Value, String> {
    let response = client
        .get(api_url(config, path))
        .basic_auth(key, Some(secret))
        .send()
        .await
        .map_err(network_error)?;

    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 | 403 => {
                "Trading 212 rejected the read-only credentials or permissions".to_string()
            }
            429 => "Trading 212 rate limit reached; try again shortly".to_string(),
            status => format!("Trading 212 returned HTTP {status}"),
        });
    }
    response
        .json()
        .await
        .map_err(|_| "Trading 212 returned an unreadable response".to_string())
}

fn api_url(config: &Config, path: &str) -> String {
    join_api_url(&config.trading212_base_url, path)
}

fn join_api_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if path.starts_with("/api/v0/") {
        let origin = base.strip_suffix("/api/v0").unwrap_or(base);
        format!("{origin}{path}")
    } else if !path.starts_with('/') {
        format!("{base}/equity/history/transactions?{path}")
    } else {
        format!("{base}{path}")
    }
}

fn transaction_from_value(value: &Value) -> Option<CashEvent> {
    Some(CashEvent {
        reference: text(value, "/reference")?.to_string(),
        event_type: text(value, "/type")?.to_string(),
        amount: value.pointer("/amount")?.as_f64()?,
        currency: text(value, "/currency")?.to_string(),
        date_time: text(value, "/dateTime")?.to_string(),
    })
}

pub async fn currency_rate(client: &Client, config: &Config, from: &str) -> Result<f64, String> {
    if from.eq_ignore_ascii_case(&config.base_currency) {
        return Ok(1.0);
    }
    let response: Value = client
        .get(format!(
            "{}/rate/{}/{}",
            config.frankfurter_base_url.trim_end_matches('/'),
            from,
            config.base_currency,
        ))
        .send()
        .await
        .map_err(network_error)?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Currency conversion failed: {}",
                error.status().map_or("unknown".into(), |s| s.to_string())
            )
        })?
        .json()
        .await
        .map_err(|_| "Currency conversion returned an unreadable response".to_string())?;
    response
        .pointer("/rate")
        .and_then(Value::as_f64)
        .ok_or_else(|| {
            format!(
                "No {from} to {} exchange rate was returned",
                config.base_currency
            )
        })
}

fn position_to_holding(position: &Value, index: usize, rate: f64) -> Holding {
    let quantity = number(position, "/quantity");
    let price = number(position, "/currentPrice") * rate;
    let value = position
        .pointer("/walletImpact/currentValue")
        .and_then(Value::as_f64)
        .unwrap_or(quantity * number(position, "/currentPrice"))
        * rate;
    let return_value = position
        .pointer("/walletImpact/unrealizedProfitLoss")
        .and_then(Value::as_f64)
        .unwrap_or(
            (number(position, "/currentPrice") - number(position, "/averagePricePaid")) * quantity,
        )
        * rate;
    let symbol = text(position, "/instrument/ticker")
        .or_else(|| text(position, "/ticker"))
        .unwrap_or("—")
        .split('_')
        .next()
        .unwrap_or("—")
        .to_string();
    let name = text(position, "/instrument/name")
        .or_else(|| text(position, "/instrument/shortName"))
        .unwrap_or(&symbol)
        .to_string();

    Holding {
        id: format!("t212-{index}-{symbol}"),
        symbol,
        name,
        source: "Trading 212".to_string(),
        quantity,
        price,
        value,
        return_value,
        allocation: 0.0,
    }
}

fn number(value: &Value, pointer: &str) -> f64 {
    value
        .pointer(pointer)
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

fn text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(Value::as_str)
}

fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "The data provider timed out".to_string()
    } else {
        "Could not reach the data provider".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::join_api_url;

    #[test]
    fn accepts_all_observed_pagination_path_shapes() {
        let base = "https://live.trading212.com/api/v0";
        assert_eq!(
            join_api_url(base, "/equity/history/transactions?limit=50"),
            "https://live.trading212.com/api/v0/equity/history/transactions?limit=50"
        );
        assert_eq!(
            join_api_url(base, "/api/v0/equity/history/transactions?limit=50"),
            "https://live.trading212.com/api/v0/equity/history/transactions?limit=50"
        );
        assert_eq!(
            join_api_url(base, "limit=50&cursor=123"),
            "https://live.trading212.com/api/v0/equity/history/transactions?limit=50&cursor=123"
        );
    }
}
