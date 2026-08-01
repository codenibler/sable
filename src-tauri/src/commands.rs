use std::sync::Mutex;

use chrono::Utc;
use reqwest::Client;
use rusqlite::Connection;
use tauri::State;

use crate::{
    config::Config,
    db,
    models::{AddWalletInput, CryptoPortfolio, Dashboard, Holding, SourceSummary},
    providers::{crypto, trading212},
};

pub struct AppState {
    pub database: Mutex<Connection>,
    pub client: Client,
    pub config: Config,
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let mut portfolios = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::list_portfolios(&database)?
    };

    let trading_result = if state.config.trading212_is_configured() {
        trading212::fetch_overview(&state.client, &state.config).await
    } else {
        Err("Add read-only Trading 212 credentials to .env".to_string())
    };

    let has_wallets = portfolios
        .iter()
        .any(|portfolio| !portfolio.wallets.is_empty());
    let price_result = if has_wallets {
        Some(crypto::prices(&state.client, &state.config).await)
    } else {
        None
    };

    if let Some(Ok(prices)) = &price_result {
        for portfolio in &mut portfolios {
            for wallet in &mut portfolio.wallets {
                crypto::hydrate_wallet(&state.client, &state.config, wallet, prices).await;
            }
        }
    } else if let Some(Err(message)) = &price_result {
        for portfolio in &mut portfolios {
            for wallet in &mut portfolio.wallets {
                wallet.message = Some(message.clone());
            }
        }
    }

    let mut notices = Vec::new();
    let mut sources = Vec::new();
    let mut holdings = Vec::new();
    let (trading_value, trading_invested, cash_value, trading_return) = match trading_result {
        Ok(overview) => {
            sources.push(SourceSummary {
                id: "trading212".to_string(),
                name: "Trading 212".to_string(),
                kind: "brokerage".to_string(),
                value: overview.total_value,
                return_value: overview.return_value,
                connected: true,
                message: None,
            });
            holdings.extend(overview.holdings);
            (
                overview.total_value,
                overview.invested_value,
                overview.cash_value,
                overview.return_value,
            )
        }
        Err(message) => {
            notices.push(message.clone());
            sources.push(SourceSummary {
                id: "trading212".to_string(),
                name: "Trading 212".to_string(),
                kind: "brokerage".to_string(),
                value: 0.0,
                return_value: 0.0,
                connected: false,
                message: Some(message),
            });
            (0.0, 0.0, 0.0, 0.0)
        }
    };

    let mut crypto_value = 0.0;
    let mut crypto_invested = 0.0;
    let mut crypto_return = 0.0;
    {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        for portfolio in &mut portfolios {
            portfolio.value = portfolio.wallets.iter().map(|wallet| wallet.value).sum();
            let baseline = db::first_snapshot_value(&database, "portfolio", portfolio.id)?
                .unwrap_or(portfolio.value);
            portfolio.return_value = portfolio.value - baseline;
            db::save_snapshot(
                &database,
                "portfolio",
                portfolio.id,
                portfolio.value,
                baseline,
                state.config.snapshot_interval_minutes,
            )?;
            crypto_value += portfolio.value;
            crypto_invested += baseline;
            crypto_return += portfolio.return_value;
            sources.push(SourceSummary {
                id: format!("portfolio-{}", portfolio.id),
                name: portfolio.name.clone(),
                kind: "crypto".to_string(),
                value: portfolio.value,
                return_value: portfolio.return_value,
                connected: portfolio
                    .wallets
                    .iter()
                    .all(|wallet| wallet.message.is_none()),
                message: portfolio
                    .wallets
                    .iter()
                    .find_map(|wallet| wallet.message.clone()),
            });
            holdings.extend(portfolio.wallets.iter().map(|wallet| Holding {
                id: format!("wallet-{}", wallet.id),
                symbol: wallet.symbol.clone(),
                name: wallet.label.clone(),
                source: portfolio.name.clone(),
                quantity: wallet.balance,
                price: if wallet.balance > 0.0 {
                    wallet.value / wallet.balance
                } else {
                    0.0
                },
                value: wallet.value,
                return_value: 0.0,
                allocation: 0.0,
            }));
        }

        let total_value = trading_value + crypto_value;
        let invested_value = trading_invested + crypto_invested;
        db::save_snapshot(
            &database,
            "total",
            0,
            total_value,
            invested_value,
            state.config.snapshot_interval_minutes,
        )?;
    }

    if has_wallets {
        notices.push(
            "Wallet balances currently include native BTC, ETH, and SOL; indexed token balances are the next adapter extension."
                .to_string(),
        );
    }

    let total_value = trading_value + crypto_value;
    let invested_value = trading_invested + crypto_invested;
    let total_return = trading_return + crypto_return;
    for holding in &mut holdings {
        holding.allocation = if total_value > 0.0 {
            holding.value / total_value * 100.0
        } else {
            0.0
        };
    }
    holdings.sort_by(|left, right| right.value.total_cmp(&left.value));

    let history = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::total_history(&database)?
    };

    Ok(Dashboard {
        total_value,
        invested_value,
        cash_value,
        total_return,
        return_percent: if invested_value > 0.0 {
            total_return / invested_value * 100.0
        } else {
            0.0
        },
        currency: state.config.base_currency.clone(),
        updated_at: Utc::now().to_rfc3339(),
        history,
        sources,
        holdings,
        portfolios,
        notices,
    })
}

#[tauri::command]
pub fn list_crypto_portfolios(state: State<'_, AppState>) -> Result<Vec<CryptoPortfolio>, String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::list_portfolios(&database)
}

#[tauri::command]
pub fn create_crypto_portfolio(state: State<'_, AppState>, name: String) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 60 {
        return Err("Portfolio name must contain 1–60 characters".to_string());
    }
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::create_portfolio(&database, name)
}

#[tauri::command]
pub fn delete_crypto_portfolio(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::delete_portfolio(&database, id)
}

#[tauri::command]
pub fn add_wallet(state: State<'_, AppState>, input: AddWalletInput) -> Result<i64, String> {
    let network = crypto::validate_address(&input.network, &input.address)?;
    let label = if input.label.trim().is_empty() {
        format!("{} wallet", network.to_uppercase())
    } else {
        input.label.trim().to_string()
    };
    if label.len() > 60 {
        return Err("Wallet label must contain at most 60 characters".to_string());
    }
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::add_wallet(
        &database,
        input.portfolio_id,
        &network,
        &input.address,
        &label,
    )
}

#[tauri::command]
pub fn remove_wallet(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::remove_wallet(&database, id)
}
