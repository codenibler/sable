mod commands;
mod config;
mod db;
mod models;
mod providers;

use std::{fs, time::Duration};

use commands::AppState;
use tauri::{Emitter, Manager};

pub fn validate_config() -> Result<(), String> {
    config::Config::load().map(|_| ())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config = config::Config::load().map_err(std::io::Error::other)?;
            let history_backfill_retry_seconds = config.history_backfill_retry_seconds;
            let data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&data_directory)?;
            let database_path = data_directory.join(&config.database_filename);
            let database = db::open(&database_path).map_err(std::io::Error::other)?;
            config::harden_private_file(&database_path).map_err(std::io::Error::other)?;
            db::import_net_worth_history(&database, &config.net_worth_history)
                .map_err(std::io::Error::other)?;
            if let Some(path) = &config.net_worth_history_path {
                let entries =
                    db::list_net_worth_entries(&database).map_err(std::io::Error::other)?;
                config::write_net_worth_history(path, &entries).map_err(std::io::Error::other)?;
            }
            let net_worth_entries =
                db::list_net_worth_entries(&database).map_err(std::io::Error::other)?;
            config::backup_net_worth_history(
                &config.net_worth_backup_directory,
                &net_worth_entries,
                config.net_worth_backup_interval_days,
            )
            .map_err(std::io::Error::other)?;
            let backup_path = db::backup_database(
                &database,
                &data_directory.join("backups"),
                config.database_backup_count,
            )
            .map_err(std::io::Error::other)?;
            config::harden_private_file(&backup_path).map_err(std::io::Error::other)?;
            let portfolio_id = db::ensure_portfolio(&database).map_err(std::io::Error::other)?;
            import_configured_wallets(&database, portfolio_id, &config)
                .map_err(std::io::Error::other)?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(config.http_timeout_seconds))
                .user_agent("Sable/0.1")
                .build()?;
            app.manage(AppState {
                database: std::sync::Mutex::new(database),
                client,
                config,
                history_sync: tokio::sync::Mutex::new(()),
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(history_backfill_retry_seconds)).await;
                    let state = app_handle.state::<AppState>();
                    if !state.config.trading212_is_configured() {
                        break;
                    }
                    let complete = state
                        .database
                        .lock()
                        .ok()
                        .and_then(|database| db::history_sync_state(&database).ok())
                        .is_some_and(|sync| sync.backfill_complete);
                    if complete {
                        break;
                    }
                    if commands::sync_trading_history(&state).await.is_ok() {
                        let _ = app_handle.emit("history-synced", ());
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::set_opessocius_monthly_return,
            commands::list_crypto_portfolios,
            commands::add_wallet,
            commands::remove_wallet,
            commands::list_net_worth_entries,
            commands::save_net_worth_entry,
            commands::remove_net_worth_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sable");
}

fn import_configured_wallets(
    database: &rusqlite::Connection,
    portfolio_id: i64,
    config: &config::Config,
) -> Result<(), String> {
    for (network, values, label) in [
        ("btc-xpub", &config.configured_bitcoin_xpubs, "Bitcoin XPUB"),
        (
            "eth",
            &config.configured_ethereum_addresses,
            "Ethereum wallet",
        ),
        ("sol", &config.configured_solana_addresses, "Solana wallet"),
        (
            "sol",
            &config.configured_staked_solana_addresses,
            "Staked Solana wallet",
        ),
    ] {
        for value in values {
            let validated = providers::crypto::validate_wallet(network, value)
                .map_err(|error| format!("Configured {label} is invalid: {error}"))?;
            match db::add_wallet(
                database,
                portfolio_id,
                &validated.network,
                value,
                label,
                &validated.wallet_type,
            ) {
                Ok(_) => {}
                Err(error) if error == "This wallet is already in the portfolio" => {}
                Err(error) => return Err(error),
            }
        }
    }
    for pool_address in &config.configured_staked_ethereum_addresses {
        let validated = providers::crypto::validate_wallet("eth", pool_address)
            .map_err(|error| format!("Configured Everstake pool is invalid: {error}"))?;
        match db::add_wallet(
            database,
            portfolio_id,
            &validated.network,
            pool_address,
            "Everstake staked ETH",
            "everstake",
        ) {
            Ok(_) => {}
            Err(error) if error == "This wallet is already in the portfolio" => {
                db::update_wallet_metadata(
                    database,
                    portfolio_id,
                    &validated.network,
                    pool_address,
                    "Everstake staked ETH",
                    "everstake",
                )?;
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
