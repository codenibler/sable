use std::{
    collections::{BTreeMap, HashMap},
    sync::Mutex,
};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use rusqlite::Connection;
use tauri::State;

use crate::{
    config::{self, Config, EthereumTokenConfig},
    db,
    models::{
        AddWalletInput, CryptoAsset, CryptoPortfolio, Dashboard, HistorySyncState, Holding,
        MonitoredPortfolio, MonthlyWinnings, NetWorthEntry, PeriodReturn, PortfolioPeriod,
        SaveNetWorthInput, SourceSummary,
    },
    providers::{crypto, trading212},
};

pub struct AppState {
    pub database: Mutex<Connection>,
    pub client: Client,
    pub config: Config,
    pub history_sync: tokio::sync::Mutex<()>,
}

const OPESSOCIUS_SOURCE: &str = "opessocius";

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<Dashboard, String> {
    let mut portfolios = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::list_portfolios(&database)?
    };

    let history_sync_error = if state.config.trading212_is_configured() {
        sync_trading_history(&state).await.err()
    } else {
        None
    };
    let cash_history = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::cash_history_summary(&database)?
    };
    let (return_month_start, return_month_label) = return_month(Local::now())?;
    let (opessocius_winnings, editable_opessocius_return) = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        if return_month_start >= state.config.opessocius_return_start_month {
            ensure_default_monthly_returns(
                &database,
                state.config.opessocius_current_balance,
                state.config.opessocius_monthly_return_rate,
                &state.config.opessocius_return_start_month,
                &return_month_start,
            )?;
        }
        let winnings = db::monthly_winnings(&database, OPESSOCIUS_SOURCE)?
            .into_iter()
            .filter(|(month, _)| month >= &state.config.opessocius_return_start_month)
            .collect::<Vec<_>>();
        let editable = if return_month_start >= state.config.opessocius_return_start_month {
            db::monthly_winning(&database, OPESSOCIUS_SOURCE, &return_month_start)?.map(
                |(amount, is_override)| MonthlyWinnings {
                    month: return_month_start.clone(),
                    label: return_month_label.clone(),
                    amount,
                    is_override,
                    default_rate_percent: state.config.opessocius_monthly_return_rate * 100.0,
                },
            )
        } else {
            None
        };
        (winnings, editable)
    };
    let total_opessocius_winnings: f64 = opessocius_winnings.iter().map(|(_, amount)| amount).sum();
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
        let mut cache_updates = Vec::new();
        for portfolio in &mut portfolios {
            for wallet in &mut portfolio.wallets {
                if crypto::hydrate_wallet(&state.client, &state.config, wallet, prices).await {
                    cache_updates.push((wallet.id, wallet.balance, wallet.address_count));
                }
            }
        }
        if !cache_updates.is_empty() {
            let database = state.database.lock().map_err(|_| "Database lock failed")?;
            for (id, balance, address_count) in cache_updates {
                db::update_wallet_cache(&database, id, balance, address_count)?;
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
    let trading_connected = trading_result.is_ok();
    let mut trading_holding_count = 0;
    if let Some(message) = history_sync_error {
        notices.push(format!("Cash history sync paused: {message}"));
    } else if !cash_history.backfill_complete && cash_history.event_count > 0 {
        notices.push(format!(
            "Trading 212 history backfill is in progress ({} events stored). Refresh later to continue.",
            cash_history.event_count
        ));
    }
    let history_is_usable = cash_history.backfill_complete && cash_history.event_count > 0;
    let (trading_value, trading_invested, cash_value, trading_return) = match trading_result {
        Ok(overview) => {
            trading_holding_count = overview.holdings.len();
            let contribution_adjusted_return = if history_is_usable {
                overview.total_value - cash_history.net_contributions
            } else {
                overview.return_value
            };
            sources.push(SourceSummary {
                id: "trading212".to_string(),
                name: "Trading 212".to_string(),
                kind: "brokerage".to_string(),
                value: overview.total_value,
                return_value: contribution_adjusted_return,
                connected: true,
                message: None,
            });
            holdings.extend(overview.holdings);
            (
                overview.total_value,
                overview.invested_value,
                overview.cash_value,
                contribution_adjusted_return,
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

    let opessocius_value = state.config.opessocius_current_balance + total_opessocius_winnings;
    let opessocius_invested = state.config.opessocius_net_deposits;
    let opessocius_return = opessocius_value - opessocius_invested;
    let opessocius_history_through = state
        .config
        .opessocius_history
        .last()
        .map(|row| month_label(&row.month))
        .transpose()?
        .unwrap_or_else(|| "baseline".to_string());
    sources.push(SourceSummary {
        id: "opessocius".to_string(),
        name: state.config.opessocius_name.clone(),
        kind: "manual".to_string(),
        value: opessocius_value,
        return_value: opessocius_return,
        connected: true,
        message: Some(format!(
            "{} · {} monthly returns recorded",
            if editable_opessocius_return
                .as_ref()
                .is_some_and(|monthly| monthly.is_override)
            {
                format!("manual {return_month_label} override")
            } else {
                format!(
                    "history through {} · {:.2}% monthly thereafter",
                    opessocius_history_through,
                    state.config.opessocius_monthly_return_rate * 100.0,
                )
            },
            opessocius_winnings.len(),
        )),
    });
    holdings.push(Holding {
        id: "manual-opessocius".to_string(),
        symbol: state
            .config
            .opessocius_name
            .chars()
            .take(2)
            .collect::<String>()
            .to_uppercase(),
        name: state.config.opessocius_name.clone(),
        source: state.config.opessocius_name.clone(),
        quantity: 1.0,
        price: opessocius_value,
        value: opessocius_value,
        return_value: opessocius_return,
        allocation: 0.0,
    });

    let mut crypto_value = 0.0;
    let mut crypto_invested = 0.0;
    let mut crypto_return = 0.0;
    {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        for portfolio in &mut portfolios {
            portfolio.value = portfolio.wallets.iter().map(|wallet| wallet.value).sum();
            portfolio.assets =
                crypto_assets(&database, portfolio, state.config.snapshot_interval_minutes)?;
            let baseline = crypto_portfolio_baseline(portfolio);
            portfolio.return_value = portfolio.value - baseline;
            db::save_snapshot(
                &database,
                "portfolio",
                portfolio.id,
                portfolio.value,
                baseline,
                0.0,
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
            holdings.extend(portfolio.wallets.iter().flat_map(|wallet| {
                wallet.assets.iter().map(|asset| Holding {
                    id: format!("wallet-{}-{}", wallet.id, asset.id),
                    symbol: asset.symbol.clone(),
                    name: format!("{} · {}", asset.name, wallet.label),
                    source: portfolio.name.clone(),
                    quantity: asset.balance,
                    price: asset.price,
                    value: asset.value,
                    return_value: 0.0,
                    allocation: 0.0,
                })
            }));
        }

        let total_value = trading_value + crypto_value + opessocius_value;
        let invested_value = if history_is_usable {
            cash_history.net_contributions + crypto_invested + opessocius_invested
        } else {
            trading_invested + crypto_invested + opessocius_invested
        };
        if trading_connected {
            db::save_snapshot(
                &database,
                "trading212",
                0,
                trading_value,
                if history_is_usable {
                    cash_history.net_contributions
                } else {
                    trading_invested
                },
                0.0,
                state.config.snapshot_interval_minutes,
            )?;
        }
        db::save_snapshot(
            &database,
            "total",
            0,
            total_value,
            invested_value,
            total_opessocius_winnings,
            state.config.snapshot_interval_minutes,
        )?;
    }

    let total_value = trading_value + crypto_value + opessocius_value;
    let invested_value = trading_invested + crypto_invested + opessocius_invested;
    let total_return = trading_return + crypto_return + opessocius_return;
    let contribution_basis = if history_is_usable {
        cash_history.net_contributions + crypto_invested + opessocius_invested
    } else {
        invested_value
    };
    for holding in &mut holdings {
        holding.allocation = if total_value > 0.0 {
            holding.value / total_value * 100.0
        } else {
            0.0
        };
    }
    holdings.sort_by(|left, right| right.value.total_cmp(&left.value));

    let local_now = Local::now();
    let month_start = Local
        .with_ymd_and_hms(local_now.year(), local_now.month(), 1, 0, 0, 0)
        .single()
        .map(|date| date.with_timezone(&Utc).to_rfc3339())
        .ok_or("Could not determine the start of the current month")?;
    let year_start = Local
        .with_ymd_and_hms(local_now.year(), 1, 1, 0, 0, 0)
        .single()
        .map(|date| date.with_timezone(&Utc).to_rfc3339())
        .ok_or("Could not determine the start of the current year")?;
    let period_end = Utc::now();
    let month_start_time = month_start
        .parse::<DateTime<Utc>>()
        .map_err(|_| "Could not parse the start of the current month")?;
    let year_start_time = year_start
        .parse::<DateTime<Utc>>()
        .map_err(|_| "Could not parse the start of the current year")?;
    let distributed_monthly =
        distributed_winnings(&opessocius_winnings, month_start_time, period_end)?;
    let distributed_yearly =
        distributed_winnings(&opessocius_winnings, year_start_time, period_end)?;
    let (mut history, monthly_return, yearly_return) = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        let monthly = db::simple_return_since(
            &database,
            &month_start,
            total_return,
            contribution_basis,
            total_opessocius_winnings,
            distributed_monthly,
        )?;
        let yearly = db::simple_return_since(
            &database,
            &year_start,
            total_return,
            contribution_basis,
            total_opessocius_winnings,
            distributed_yearly,
        )?;
        (
            db::total_history(&database)?,
            PeriodReturn {
                amount: monthly.0,
                percent: monthly.1,
            },
            PeriodReturn {
                amount: yearly.0,
                percent: yearly.1,
            },
        )
    };
    for point in &mut history {
        let timestamp = point
            .timestamp
            .parse::<DateTime<Utc>>()
            .map_err(|_| "Stored portfolio history contains an invalid timestamp")?;
        let accrued = accrued_winnings(&opessocius_winnings, timestamp)?;
        point.value = point.value - point.opessocius_winnings + accrued;
    }
    include_configured_tokens_from_portfolio_start(
        &mut history,
        &portfolios,
        &state.config.ethereum_tokens,
        state.config.snapshot_interval_minutes,
    );

    let (opessocius_history, opessocius_periods) =
        opessocius_portfolio_history(&state.config, &opessocius_winnings)?;
    let trading_history = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::source_history(&database, "trading212", 0)?
    };
    let trading_basis = if history_is_usable {
        cash_history.net_contributions
    } else {
        trading_invested
    };
    let mut monitored_portfolios = vec![
        MonitoredPortfolio {
            id: "trading212".to_string(),
            name: "Trading 212".to_string(),
            kind: "brokerage".to_string(),
            value: trading_value,
            invested_value: trading_basis,
            total_return: trading_return,
            return_percent: percent_of(trading_return, trading_basis),
            history: trading_history,
            periods: Vec::new(),
            item_count: trading_holding_count,
            item_label: "holdings".to_string(),
            connected: trading_connected,
            message: sources
                .iter()
                .find(|source| source.id == "trading212")
                .and_then(|source| source.message.clone()),
        },
        MonitoredPortfolio {
            id: "opessocius".to_string(),
            name: state.config.opessocius_name.clone(),
            kind: "manual".to_string(),
            value: opessocius_value,
            invested_value: opessocius_invested,
            total_return: opessocius_return,
            return_percent: percent_of(opessocius_return, opessocius_invested),
            history: opessocius_history,
            item_count: opessocius_periods.len(),
            item_label: "monthly records".to_string(),
            periods: opessocius_periods,
            connected: true,
            message: Some("Authoritative local history".to_string()),
        },
    ];
    {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        for portfolio in &mut portfolios {
            let invested = portfolio.value - portfolio.return_value;
            let mut portfolio_history = db::source_history(&database, "portfolio", portfolio.id)?;
            include_configured_tokens_from_portfolio_start(
                &mut portfolio_history,
                std::slice::from_ref(portfolio),
                &state.config.ethereum_tokens,
                state.config.snapshot_interval_minutes,
            );
            normalize_crypto_invested_history(&mut portfolio_history, invested);
            if let Some(started_at) = portfolio_history
                .first()
                .map(|point| point.timestamp.clone())
            {
                for asset in portfolio
                    .assets
                    .iter_mut()
                    .filter(|asset| is_configured_token(asset, &state.config.ethereum_tokens))
                {
                    extend_asset_history_to(&mut asset.history, &started_at);
                }
            }
            monitored_portfolios.push(MonitoredPortfolio {
                id: format!("portfolio-{}", portfolio.id),
                name: portfolio.name.clone(),
                kind: "crypto".to_string(),
                value: portfolio.value,
                invested_value: invested,
                total_return: portfolio.return_value,
                return_percent: percent_of(portfolio.return_value, invested),
                history: portfolio_history,
                periods: Vec::new(),
                item_count: portfolio.wallets.len(),
                item_label: "wallets".to_string(),
                connected: portfolio
                    .wallets
                    .iter()
                    .all(|wallet| wallet.message.is_none()),
                message: portfolio
                    .wallets
                    .iter()
                    .find_map(|wallet| wallet.message.clone()),
            });
        }
    }

    Ok(Dashboard {
        total_value,
        invested_value,
        cash_value,
        total_return,
        return_percent: if contribution_basis > 0.0 {
            total_return / contribution_basis * 100.0
        } else {
            0.0
        },
        monthly_return,
        yearly_return,
        opessocius_monthly_return: editable_opessocius_return,
        net_contributions: cash_history.net_contributions + opessocius_invested,
        history_event_count: cash_history.event_count,
        history_backfill_complete: cash_history.backfill_complete,
        refresh_interval_minutes: state.config.snapshot_interval_minutes.max(1),
        currency: state.config.base_currency.clone(),
        updated_at: Utc::now().to_rfc3339(),
        history,
        sources,
        holdings,
        portfolios,
        monitored_portfolios,
        notices,
    })
}

fn percent_of(amount: f64, basis: f64) -> f64 {
    if basis > 0.0 {
        amount / basis * 100.0
    } else {
        0.0
    }
}

fn crypto_portfolio_baseline(portfolio: &CryptoPortfolio) -> f64 {
    portfolio
        .assets
        .iter()
        .map(|asset| asset.invested_value)
        .sum()
}

fn crypto_assets(
    connection: &rusqlite::Connection,
    portfolio: &CryptoPortfolio,
    snapshot_interval_minutes: i64,
) -> Result<Vec<CryptoAsset>, String> {
    let grouped = grouped_crypto_assets(portfolio);
    let mut assets = Vec::new();
    for (id, (network, symbol, name, balance, value, wallet_count)) in grouped {
        let source_kind = format!("crypto-{id}");
        let invested_value =
            db::snapshot_baseline(connection, &source_kind, portfolio.id)?.unwrap_or(value);
        db::save_snapshot(
            connection,
            &source_kind,
            portfolio.id,
            value,
            invested_value,
            0.0,
            snapshot_interval_minutes,
        )?;
        let total_return = value - invested_value;
        assets.push(CryptoAsset {
            id,
            network,
            symbol,
            name,
            balance,
            value,
            invested_value,
            total_return,
            return_percent: percent_of(total_return, invested_value),
            allocation: if portfolio.value > 0.0 {
                value / portfolio.value * 100.0
            } else {
                0.0
            },
            wallet_count,
            history: db::source_history(connection, &source_kind, portfolio.id)?,
        });
    }
    assets.sort_by_key(|asset| match asset.id.as_str() {
        "btc" => 0,
        "eth" => 1,
        "link" => 2,
        "sol" => 3,
        _ => 4,
    });
    Ok(assets)
}

type GroupedCryptoAssets = BTreeMap<String, (String, String, String, f64, f64, usize)>;

fn grouped_crypto_assets(portfolio: &CryptoPortfolio) -> GroupedCryptoAssets {
    let mut grouped = BTreeMap::<String, (String, String, String, f64, f64, usize)>::new();
    for wallet in &portfolio.wallets {
        for asset in &wallet.assets {
            let entry = grouped.entry(asset.id.clone()).or_insert_with(|| {
                (
                    asset.network.clone(),
                    asset.symbol.clone(),
                    asset.name.clone(),
                    0.0,
                    0.0,
                    0,
                )
            });
            entry.3 += asset.balance;
            entry.4 += asset.value;
            entry.5 += 1;
        }
    }
    grouped
}

fn include_configured_tokens_from_portfolio_start(
    history: &mut [crate::models::DataPoint],
    portfolios: &[CryptoPortfolio],
    tokens: &[EthereumTokenConfig],
    snapshot_interval_minutes: i64,
) {
    for asset in portfolios
        .iter()
        .flat_map(|portfolio| &portfolio.assets)
        .filter(|asset| is_configured_token(asset, tokens))
    {
        let Some(first_asset_point) = asset.history.first() else {
            continue;
        };
        let Ok(first_asset_time) = first_asset_point.timestamp.parse::<DateTime<Utc>>() else {
            continue;
        };
        let before_capture_window =
            first_asset_time - Duration::minutes(snapshot_interval_minutes.max(1));
        for point in history.iter_mut().filter(|point| {
            point
                .timestamp
                .parse::<DateTime<Utc>>()
                .is_ok_and(|timestamp| timestamp <= before_capture_window)
        }) {
            point.value += first_asset_point.value;
            point.invested += first_asset_point.invested;
        }
    }
}

fn is_configured_token(asset: &CryptoAsset, tokens: &[EthereumTokenConfig]) -> bool {
    tokens
        .iter()
        .any(|token| token.symbol.eq_ignore_ascii_case(&asset.symbol))
}

fn normalize_crypto_invested_history(history: &mut [crate::models::DataPoint], invested: f64) {
    for point in history {
        point.invested = invested;
    }
}

fn extend_asset_history_to(history: &mut Vec<crate::models::DataPoint>, started_at: &str) {
    let Some(first_point) = history.first() else {
        return;
    };
    if started_at >= first_point.timestamp.as_str() {
        return;
    }
    history.insert(
        0,
        crate::models::DataPoint {
            timestamp: started_at.to_string(),
            value: first_point.value,
            invested: first_point.invested,
            opessocius_winnings: 0.0,
        },
    );
}

fn opessocius_portfolio_history(
    config: &Config,
    automatic_returns: &[(String, f64)],
) -> Result<(Vec<crate::models::DataPoint>, Vec<PortfolioPeriod>), String> {
    let mut history = Vec::new();
    let mut periods = Vec::new();
    let mut invested = config
        .opessocius_history
        .first()
        .map(|row| row.ending_balance_eur - row.return_eur - row.deposits_eur + row.withdrawals_eur)
        .unwrap_or(config.opessocius_net_deposits);

    for row in &config.opessocius_history {
        invested += row.deposits_eur - row.withdrawals_eur;
        history.push(crate::models::DataPoint {
            timestamp: month_end_timestamp(&row.month)?,
            value: row.ending_balance_eur,
            invested,
            opessocius_winnings: 0.0,
        });
        periods.push(PortfolioPeriod {
            month: row.month.clone(),
            label: month_label(&row.month)?,
            return_percent: row.return_percent,
            return_value: row.return_eur,
            deposits: row.deposits_eur,
            withdrawals: row.withdrawals_eur,
            ending_value: row.ending_balance_eur,
        });
    }

    let mut value = config.opessocius_current_balance;
    for (month, amount) in automatic_returns {
        let opening = value;
        value += amount;
        history.push(crate::models::DataPoint {
            timestamp: month_end_timestamp(month)?,
            value,
            invested: config.opessocius_net_deposits,
            opessocius_winnings: 0.0,
        });
        periods.push(PortfolioPeriod {
            month: month.clone(),
            label: month_label(month)?,
            return_percent: percent_of(*amount, opening),
            return_value: *amount,
            deposits: 0.0,
            withdrawals: 0.0,
            ending_value: value,
        });
    }
    Ok((history, periods))
}

fn month_end_timestamp(month: &str) -> Result<String, String> {
    let (_, end) = month_bounds(month)?;
    Ok((end - Duration::seconds(1)).to_rfc3339())
}

fn month_label(month: &str) -> Result<String, String> {
    NaiveDate::parse_from_str(month, "%Y-%m-%d")
        .map(|date| date.format("%B %Y").to_string())
        .map_err(|_| "Stored Opessocius history contains an invalid month".to_string())
}

pub(crate) async fn sync_trading_history(state: &AppState) -> Result<(), String> {
    let _sync_guard = state.history_sync.lock().await;
    let sync_state = {
        let database = state.database.lock().map_err(|_| "Database lock failed")?;
        db::history_sync_state(&database)?
    };
    if !history_sync_is_due(&sync_state, state.config.history_sync_interval_minutes) {
        return Ok(());
    }

    let incremental = sync_state.backfill_complete;
    let mut path = if incremental {
        "/equity/history/transactions?limit=50".to_string()
    } else {
        sync_state
            .next_path
            .unwrap_or_else(|| "/equity/history/transactions?limit=50".to_string())
    };
    let page_limit = if incremental {
        1
    } else {
        state.config.trading212_history_max_pages
    };

    for _ in 0..page_limit {
        let page = trading212::fetch_transaction_page(&state.client, &state.config, &path).await?;
        let mut rates = HashMap::new();
        let mut converted = Vec::with_capacity(page.events.len());
        for event in page.events {
            let rate = if let Some(rate) = rates.get(&event.currency) {
                *rate
            } else {
                let rate = trading212::currency_rate(&state.client, &state.config, &event.currency)
                    .await?;
                rates.insert(event.currency.clone(), rate);
                rate
            };
            converted.push((event, rate));
        }

        let next_path = if incremental { None } else { page.next_path };
        let complete = incremental || next_path.is_none();
        {
            let database = state.database.lock().map_err(|_| "Database lock failed")?;
            db::save_cash_events(&database, &converted)?;
            db::save_history_sync_state(&database, next_path.as_deref(), complete)?;
        }
        if complete {
            break;
        }
        path = next_path.expect("incomplete history page must have a next path");
    }
    Ok(())
}

fn history_sync_is_due(state: &HistorySyncState, interval_minutes: i64) -> bool {
    if !state.backfill_complete {
        return true;
    }
    state
        .last_synced_at
        .as_deref()
        .and_then(|value| value.parse::<DateTime<Utc>>().ok())
        .is_none_or(|last_sync| Utc::now() - last_sync >= Duration::minutes(interval_minutes))
}

fn return_month(now: DateTime<Local>) -> Result<(String, String), String> {
    let current_month = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
        .ok_or("Could not determine the current month")?;
    let next_month = next_month_start(current_month)?;
    let final_day = next_month - Duration::days(1);
    let start = if now.date_naive() == final_day {
        current_month
    } else {
        let previous = current_month - Duration::days(1);
        NaiveDate::from_ymd_opt(previous.year(), previous.month(), 1)
            .ok_or("Could not determine the previous month")?
    };
    Ok((
        start.format("%Y-%m-%d").to_string(),
        start.format("%B %Y").to_string(),
    ))
}

fn next_month_start(month: NaiveDate) -> Result<NaiveDate, String> {
    let (year, month_number) = if month.month() == 12 {
        (month.year() + 1, 1)
    } else {
        (month.year(), month.month() + 1)
    };
    NaiveDate::from_ymd_opt(year, month_number, 1)
        .ok_or_else(|| "Could not determine the next month".to_string())
}

fn planned_default_monthly_returns(
    existing: &[(String, f64)],
    opening_balance: f64,
    monthly_rate: f64,
    start_month: &str,
    through_month: &str,
) -> Result<Vec<(String, f64)>, String> {
    let start = NaiveDate::parse_from_str(start_month, "%Y-%m-%d")
        .map_err(|_| "Could not determine the Opessocius return start month")?;
    let through = NaiveDate::parse_from_str(through_month, "%Y-%m-%d")
        .map_err(|_| "Could not determine the latest Opessocius return month")?;
    let mut recorded = BTreeMap::new();
    for (month, amount) in existing {
        let date = NaiveDate::parse_from_str(month, "%Y-%m-%d")
            .map_err(|_| "Stored monthly winnings contain an invalid month")?;
        if date >= start {
            recorded.insert(date, *amount);
        }
    }

    let mut month = start;
    let mut balance = opening_balance;
    let mut planned = Vec::new();
    while month <= through {
        if let Some(amount) = recorded.get(&month) {
            balance += amount;
        } else {
            let amount = (balance * monthly_rate * 100.0).round() / 100.0;
            planned.push((month.format("%Y-%m-%d").to_string(), amount));
            balance += amount;
        }
        month = next_month_start(month)?;
    }
    Ok(planned)
}

fn ensure_default_monthly_returns(
    connection: &rusqlite::Connection,
    opening_balance: f64,
    monthly_rate: f64,
    start_month: &str,
    through_month: &str,
) -> Result<(), String> {
    let existing = db::monthly_winnings(connection, OPESSOCIUS_SOURCE)?;
    for (month, amount) in planned_default_monthly_returns(
        &existing,
        opening_balance,
        monthly_rate,
        start_month,
        through_month,
    )? {
        db::save_monthly_winnings(connection, OPESSOCIUS_SOURCE, &month, amount, false)?;
    }
    Ok(())
}

fn month_bounds(month_start: &str) -> Result<(DateTime<Utc>, DateTime<Utc>), String> {
    let start_date = NaiveDate::parse_from_str(month_start, "%Y-%m-%d")
        .map_err(|_| "Stored monthly winnings contain an invalid month")?;
    let (next_year, next_month) = if start_date.month() == 12 {
        (start_date.year() + 1, 1)
    } else {
        (start_date.year(), start_date.month() + 1)
    };
    let end_date = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .ok_or("Stored monthly winnings contain an invalid month")?;
    let start = Local
        .from_local_datetime(
            &start_date
                .and_hms_opt(0, 0, 0)
                .ok_or("Could not determine monthly winnings start")?,
        )
        .single()
        .ok_or("Could not determine monthly winnings start")?
        .with_timezone(&Utc);
    let end = Local
        .from_local_datetime(
            &end_date
                .and_hms_opt(0, 0, 0)
                .ok_or("Could not determine monthly winnings end")?,
        )
        .single()
        .ok_or("Could not determine monthly winnings end")?
        .with_timezone(&Utc);
    Ok((start, end))
}

fn accrued_winnings(entries: &[(String, f64)], at: DateTime<Utc>) -> Result<f64, String> {
    entries.iter().try_fold(0.0, |total, (month, amount)| {
        Ok(total + accrued_monthly_winnings(month, *amount, at)?)
    })
}

fn distributed_winnings(
    entries: &[(String, f64)],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<f64, String> {
    entries.iter().try_fold(0.0, |total, (month, amount)| {
        Ok(total + accrued_monthly_winnings(month, *amount, end)?
            - accrued_monthly_winnings(month, *amount, start)?)
    })
}

fn accrued_monthly_winnings(month: &str, amount: f64, at: DateTime<Utc>) -> Result<f64, String> {
    let (start, end) = month_bounds(month)?;
    if at <= start {
        return Ok(0.0);
    }
    if at >= end {
        return Ok(amount);
    }
    let elapsed = (at - start).num_seconds() as f64;
    let duration = (end - start).num_seconds() as f64;
    Ok(amount * elapsed / duration)
}

#[tauri::command]
pub fn set_opessocius_monthly_return(
    state: State<'_, AppState>,
    amount: f64,
) -> Result<(), String> {
    if !amount.is_finite() {
        return Err("Monthly return must be a valid amount".to_string());
    }
    let (month_start, _) = return_month(Local::now())?;
    if month_start < state.config.opessocius_return_start_month {
        return Err("There is no automatic Opessocius return to override yet".to_string());
    }
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::save_monthly_winnings(&database, OPESSOCIUS_SOURCE, &month_start, amount, true)
}

#[tauri::command]
pub fn list_crypto_portfolios(state: State<'_, AppState>) -> Result<Vec<CryptoPortfolio>, String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::list_portfolios(&database)
}

#[tauri::command]
pub fn add_wallet(state: State<'_, AppState>, input: AddWalletInput) -> Result<i64, String> {
    let validated = crypto::validate_wallet(&input.network, &input.address)?;
    let label = if input.label.trim().is_empty() {
        if validated.wallet_type == "xpub" {
            "Bitcoin XPUB".to_string()
        } else {
            format!("{} wallet", validated.network.to_uppercase())
        }
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
        &validated.network,
        &input.address,
        &label,
        &validated.wallet_type,
    )
}

#[tauri::command]
pub fn remove_wallet(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::remove_wallet(&database, id)
}

#[tauri::command]
pub fn list_net_worth_entries(state: State<'_, AppState>) -> Result<Vec<NetWorthEntry>, String> {
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::list_net_worth_entries(&database)
}

#[tauri::command]
pub fn save_net_worth_entry(
    state: State<'_, AppState>,
    input: SaveNetWorthInput,
) -> Result<(), String> {
    let parsed_date = NaiveDate::parse_from_str(&input.date, "%Y-%m-%d")
        .map_err(|_| "Net worth date must use YYYY-MM-DD format".to_string())?;
    if parsed_date.format("%Y-%m-%d").to_string() != input.date {
        return Err("Net worth date must use YYYY-MM-DD format".to_string());
    }
    for (label, amount) in [
        ("Stocks", input.stocks),
        ("Opessocius", input.opessocius),
        ("Crypto", input.crypto),
        ("Savings", input.savings),
        ("Spending", input.spending),
        ("Receivables", input.receivables),
        ("Cash", input.cash),
        ("Misc", input.misc),
    ] {
        if !amount.is_finite() || amount < 0.0 {
            return Err(format!("{label} must be a non-negative amount"));
        }
    }
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::save_net_worth_entry(&database, &input)?;
    mirror_net_worth_history(&state, &database)
}

#[tauri::command]
pub fn remove_net_worth_entry(state: State<'_, AppState>, date: String) -> Result<(), String> {
    NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|_| "Net worth date must use YYYY-MM-DD format".to_string())?;
    let database = state.database.lock().map_err(|_| "Database lock failed")?;
    db::remove_net_worth_entry(&database, &date)?;
    mirror_net_worth_history(&state, &database)
}

fn mirror_net_worth_history(state: &AppState, database: &Connection) -> Result<(), String> {
    let entries = db::list_net_worth_entries(database)?;
    if let Some(path) = &state.config.net_worth_history_path {
        config::write_net_worth_history(path, &entries)?;
    }
    config::backup_net_worth_history(
        &state.config.net_worth_backup_directory,
        &entries,
        state.config.net_worth_backup_interval_days,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        accrued_monthly_winnings, crypto_portfolio_baseline, distributed_winnings,
        extend_asset_history_to, grouped_crypto_assets,
        include_configured_tokens_from_portfolio_start, month_bounds,
        normalize_crypto_invested_history, planned_default_monthly_returns, return_month,
    };
    use crate::{
        config::EthereumTokenConfig,
        models::{CryptoAsset, CryptoPortfolio, DataPoint, Wallet, WalletAsset},
    };
    use chrono::{Local, TimeZone};

    #[test]
    fn identifies_the_previous_calendar_month() {
        let now = Local
            .with_ymd_and_hms(2026, 1, 15, 12, 0, 0)
            .single()
            .expect("local date");
        let (month, label) = return_month(now).unwrap();
        assert_eq!(month, "2025-12-01");
        assert_eq!(label, "December 2025");
    }

    #[test]
    fn applies_the_current_month_on_its_final_calendar_day() {
        for (year, month, day) in [(2026, 2, 28), (2026, 4, 30), (2026, 8, 31)] {
            let now = Local
                .with_ymd_and_hms(year, month, day, 12, 0, 0)
                .single()
                .expect("local date");
            let (return_month, _) = return_month(now).unwrap();
            assert_eq!(return_month, format!("{year:04}-{month:02}-01"));
        }
    }

    #[test]
    fn compounds_missing_months_at_the_configured_default_rate() {
        let existing = vec![("2026-07-01".to_string(), 20.0)];
        let planned =
            planned_default_monthly_returns(&existing, 1_000.0, 0.02, "2026-07-01", "2026-09-01")
                .unwrap();
        assert_eq!(
            planned,
            vec![
                ("2026-08-01".to_string(), 20.4),
                ("2026-09-01".to_string(), 20.81),
            ]
        );
    }

    #[test]
    fn starts_automatic_returns_after_the_authoritative_history() {
        let planned =
            planned_default_monthly_returns(&[], 1_000.0, 0.02, "2026-08-01", "2026-09-01")
                .unwrap();
        assert_eq!(
            planned,
            vec![
                ("2026-08-01".to_string(), 20.0),
                ("2026-09-01".to_string(), 20.4),
            ]
        );

        assert!(
            planned_default_monthly_returns(&[], 1_000.0, 0.02, "2026-08-01", "2026-07-01",)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn distributes_winnings_across_the_full_month() {
        let (start, end) = month_bounds("2026-07-01").unwrap();
        let midpoint = start + (end - start) / 2;
        let accrued = accrued_monthly_winnings("2026-07-01", 310.0, midpoint).unwrap();
        assert!((accrued - 155.0).abs() < 0.000_001);
        let loss = accrued_monthly_winnings("2026-07-01", -100.0, midpoint).unwrap();
        assert!((loss + 50.0).abs() < 0.000_001);

        let entries = vec![("2026-07-01".to_string(), 310.0)];
        assert_eq!(distributed_winnings(&entries, start, end).unwrap(), 310.0);
        assert_eq!(distributed_winnings(&entries, end, end).unwrap(), 0.0);
    }

    #[test]
    fn separates_eth_and_link_held_by_the_same_wallet() {
        let wallet = Wallet {
            id: 1,
            portfolio_id: 1,
            network: "eth".to_string(),
            address: "0x0000000000000000000000000000000000000000".to_string(),
            display_address: "0x0000".to_string(),
            label: "Trezor Ethereum".to_string(),
            wallet_type: "address".to_string(),
            address_count: 1,
            balance: 2.0,
            symbol: "ETH".to_string(),
            value: 320.0,
            assets: vec![
                WalletAsset {
                    id: "eth".to_string(),
                    network: "eth".to_string(),
                    symbol: "ETH".to_string(),
                    name: "Ethereum".to_string(),
                    balance: 2.0,
                    price: 100.0,
                    value: 200.0,
                    message: None,
                },
                WalletAsset {
                    id: "link".to_string(),
                    network: "eth".to_string(),
                    symbol: "LINK".to_string(),
                    name: "Chainlink".to_string(),
                    balance: 10.0,
                    price: 12.0,
                    value: 120.0,
                    message: None,
                },
            ],
            message: None,
            last_checked_at: None,
        };
        let grouped = grouped_crypto_assets(&CryptoPortfolio {
            id: 1,
            name: "Trezor Safe".to_string(),
            value: 320.0,
            return_value: 0.0,
            assets: Vec::new(),
            wallets: vec![wallet],
        });
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped["eth"].3, 2.0);
        assert_eq!(grouped["eth"].4, 200.0);
        assert_eq!(grouped["link"].3, 10.0);
        assert_eq!(grouped["link"].4, 120.0);
    }

    #[test]
    fn treats_configured_tokens_as_part_of_the_original_crypto_basis() {
        let first_timestamp = "2026-07-01T00:00:00+00:00";
        let same_capture_timestamp = "2026-07-31T23:59:59+00:00";
        let detected_timestamp = "2026-08-01T00:00:00+00:00";
        let link = CryptoAsset {
            id: "link".to_string(),
            network: "eth".to_string(),
            symbol: "LINK".to_string(),
            name: "Chainlink".to_string(),
            balance: 10.0,
            value: 120.0,
            invested_value: 120.0,
            total_return: 0.0,
            return_percent: 0.0,
            allocation: 37.5,
            wallet_count: 1,
            history: vec![DataPoint {
                timestamp: detected_timestamp.to_string(),
                value: 120.0,
                invested: 120.0,
                opessocius_winnings: 0.0,
            }],
        };
        let portfolio = CryptoPortfolio {
            id: 1,
            name: "Trezor Safe".to_string(),
            value: 320.0,
            return_value: 0.0,
            assets: vec![
                CryptoAsset {
                    id: "eth".to_string(),
                    network: "eth".to_string(),
                    symbol: "ETH".to_string(),
                    name: "Ethereum".to_string(),
                    balance: 2.0,
                    value: 200.0,
                    invested_value: 200.0,
                    total_return: 0.0,
                    return_percent: 0.0,
                    allocation: 62.5,
                    wallet_count: 1,
                    history: Vec::new(),
                },
                link.clone(),
            ],
            wallets: Vec::new(),
        };
        let tokens = vec![EthereumTokenConfig {
            price_id: "chainlink".to_string(),
            symbol: "LINK".to_string(),
            name: "Chainlink".to_string(),
            contract_address: "0x514910771AF9Ca656af840dff83E8264EcF986CA".to_string(),
            decimals: 18,
        }];
        let mut combined_history = vec![
            DataPoint {
                timestamp: first_timestamp.to_string(),
                value: 200.0,
                invested: 200.0,
                opessocius_winnings: 0.0,
            },
            DataPoint {
                timestamp: same_capture_timestamp.to_string(),
                value: 320.0,
                invested: 200.0,
                opessocius_winnings: 0.0,
            },
            DataPoint {
                timestamp: detected_timestamp.to_string(),
                value: 320.0,
                invested: 320.0,
                opessocius_winnings: 0.0,
            },
        ];

        assert_eq!(crypto_portfolio_baseline(&portfolio), 320.0);
        include_configured_tokens_from_portfolio_start(
            &mut combined_history,
            std::slice::from_ref(&portfolio),
            &tokens,
            60,
        );
        normalize_crypto_invested_history(&mut combined_history, 320.0);
        assert_eq!(combined_history[0].value, 320.0);
        assert_eq!(combined_history[0].invested, 320.0);
        assert_eq!(combined_history[1].value, 320.0);
        assert_eq!(combined_history[1].invested, 320.0);
        assert_eq!(combined_history[2].value, 320.0);
        assert_eq!(
            combined_history
                .iter()
                .map(|point| point.value)
                .fold(0.0_f64, f64::max),
            320.0
        );

        let mut link_history = link.history;
        extend_asset_history_to(&mut link_history, first_timestamp);
        assert_eq!(link_history[0].timestamp, first_timestamp);
        assert_eq!(link_history[0].value, 120.0);
        assert_eq!(link_history[1].timestamp, detected_timestamp);
    }
}
