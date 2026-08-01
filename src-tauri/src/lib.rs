mod commands;
mod config;
mod db;
mod models;
mod providers;

use std::{fs, time::Duration};

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let config = config::Config::load().map_err(std::io::Error::other)?;
            let data_directory = app.path().app_data_dir()?;
            fs::create_dir_all(&data_directory)?;
            let database = db::open(&data_directory.join(&config.database_filename))
                .map_err(std::io::Error::other)?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(config.http_timeout_seconds))
                .user_agent("Portfolio-1/0.1")
                .build()?;
            app.manage(AppState {
                database: std::sync::Mutex::new(database),
                client,
                config,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::list_crypto_portfolios,
            commands::create_crypto_portfolio,
            commands::delete_crypto_portfolio,
            commands::add_wallet,
            commands::remove_wallet,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Portfolio 1");
}
