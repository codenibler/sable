use std::path::Path;

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::models::{CryptoPortfolio, DataPoint, Wallet};

pub fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(to_string)?;
    initialize(&connection)?;
    Ok(connection)
}

fn initialize(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS portfolios (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS wallets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                portfolio_id INTEGER NOT NULL REFERENCES portfolios(id) ON DELETE CASCADE,
                network TEXT NOT NULL,
                address TEXT NOT NULL,
                label TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(network, address, portfolio_id)
             );
             CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_kind TEXT NOT NULL,
                source_id INTEGER NOT NULL DEFAULT 0,
                captured_at TEXT NOT NULL,
                value_eur REAL NOT NULL,
                invested_eur REAL NOT NULL
             );
             CREATE INDEX IF NOT EXISTS snapshots_source_time
                ON snapshots(source_kind, source_id, captured_at);",
        )
        .map_err(to_string)?;
    Ok(())
}

pub fn list_portfolios(connection: &Connection) -> Result<Vec<CryptoPortfolio>, String> {
    let mut statement = connection
        .prepare("SELECT id, name FROM portfolios ORDER BY created_at, id")
        .map_err(to_string)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(to_string)?;

    let mut portfolios = Vec::new();
    for row in rows {
        let (id, name) = row.map_err(to_string)?;
        portfolios.push(CryptoPortfolio {
            id,
            name,
            value: 0.0,
            return_value: 0.0,
            wallets: list_wallets(connection, id)?,
        });
    }
    Ok(portfolios)
}

fn list_wallets(connection: &Connection, portfolio_id: i64) -> Result<Vec<Wallet>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, portfolio_id, network, address, label
             FROM wallets WHERE portfolio_id = ?1 ORDER BY created_at, id",
        )
        .map_err(to_string)?;
    let rows = statement
        .query_map([portfolio_id], |row| {
            let network: String = row.get(2)?;
            Ok(Wallet {
                id: row.get(0)?,
                portfolio_id: row.get(1)?,
                symbol: network.to_uppercase(),
                network,
                address: row.get(3)?,
                label: row.get(4)?,
                balance: 0.0,
                value: 0.0,
                message: None,
            })
        })
        .map_err(to_string)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(to_string)
}

pub fn create_portfolio(connection: &Connection, name: &str) -> Result<i64, String> {
    connection
        .execute(
            "INSERT INTO portfolios(name, created_at) VALUES (?1, ?2)",
            params![name.trim(), Utc::now().to_rfc3339()],
        )
        .map_err(to_string)?;
    Ok(connection.last_insert_rowid())
}

pub fn delete_portfolio(connection: &Connection, id: i64) -> Result<(), String> {
    connection
        .execute("DELETE FROM portfolios WHERE id = ?1", [id])
        .map_err(to_string)?;
    Ok(())
}

pub fn add_wallet(
    connection: &Connection,
    portfolio_id: i64,
    network: &str,
    address: &str,
    label: &str,
) -> Result<i64, String> {
    connection
        .execute(
            "INSERT INTO wallets(portfolio_id, network, address, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                portfolio_id,
                network,
                address.trim(),
                label.trim(),
                Utc::now().to_rfc3339()
            ],
        )
        .map_err(|error| {
            if error.to_string().contains("UNIQUE constraint failed") {
                "This wallet is already in the portfolio".to_string()
            } else {
                error.to_string()
            }
        })?;
    Ok(connection.last_insert_rowid())
}

pub fn remove_wallet(connection: &Connection, id: i64) -> Result<(), String> {
    connection
        .execute("DELETE FROM wallets WHERE id = ?1", [id])
        .map_err(to_string)?;
    Ok(())
}

pub fn save_snapshot(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
    value: f64,
    invested: f64,
    interval_minutes: i64,
) -> Result<(), String> {
    let last: Option<(i64, String)> = connection
        .query_row(
            "SELECT id, captured_at FROM snapshots
             WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY captured_at DESC LIMIT 1",
            params![source_kind, source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(to_string)?;

    let recent_id = last.and_then(|(id, value)| {
        value
            .parse::<chrono::DateTime<Utc>>()
            .ok()
            .filter(|time| Utc::now() - *time < Duration::minutes(interval_minutes))
            .map(|_| id)
    });

    if let Some(id) = recent_id {
        connection
            .execute(
                "UPDATE snapshots SET captured_at = ?1, value_eur = ?2, invested_eur = ?3
                 WHERE id = ?4",
                params![Utc::now().to_rfc3339(), value, invested, id],
            )
            .map_err(to_string)?;
    } else {
        connection
            .execute(
                "INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur, invested_eur)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![source_kind, source_id, Utc::now().to_rfc3339(), value, invested],
            )
            .map_err(to_string)?;
    }
    Ok(())
}

pub fn total_history(connection: &Connection) -> Result<Vec<DataPoint>, String> {
    let mut statement = connection
        .prepare(
            "SELECT captured_at, value_eur, invested_eur FROM snapshots
             WHERE source_kind = 'total' ORDER BY captured_at DESC LIMIT 500",
        )
        .map_err(to_string)?;
    let mut points = statement
        .query_map([], |row| {
            Ok(DataPoint {
                timestamp: row.get(0)?,
                value: row.get(1)?,
                invested: row.get(2)?,
            })
        })
        .map_err(to_string)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_string)?;
    points.reverse();
    Ok(points)
}

pub fn first_snapshot_value(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<Option<f64>, String> {
    connection
        .query_row(
            "SELECT value_eur FROM snapshots WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY captured_at LIMIT 1",
            params![source_kind, source_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_string)
}

fn to_string(error: rusqlite::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{add_wallet, create_portfolio, initialize, list_portfolios, save_snapshot};

    #[test]
    fn stores_grouped_wallets() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let portfolio_id = create_portfolio(&database, "Cold storage").expect("portfolio");
        add_wallet(
            &database,
            portfolio_id,
            "eth",
            "0x0000000000000000000000000000000000000000",
            "Ledger",
        )
        .expect("wallet");

        let portfolios = list_portfolios(&database).expect("portfolio list");
        assert_eq!(portfolios.len(), 1);
        assert_eq!(portfolios[0].wallets.len(), 1);
        assert_eq!(portfolios[0].wallets[0].label, "Ledger");
    }

    #[test]
    fn refreshes_the_current_snapshot_bucket() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "total", 0, 10.0, 8.0, 60).expect("first snapshot");
        save_snapshot(&database, "total", 0, 12.0, 8.0, 60).expect("updated snapshot");

        let (count, value): (i64, f64) = database
            .query_row(
                "SELECT COUNT(*), MAX(value_eur) FROM snapshots WHERE source_kind = 'total'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("snapshot query");
        assert_eq!(count, 1);
        assert_eq!(value, 12.0);
    }
}
