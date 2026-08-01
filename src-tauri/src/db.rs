use std::path::Path;

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::models::{
    CashEvent, CashHistorySummary, CryptoPortfolio, DataPoint, HistorySyncState, Wallet,
};

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
                wallet_type TEXT NOT NULL DEFAULT 'address',
                cached_balance REAL NOT NULL DEFAULT 0,
                cached_address_count INTEGER NOT NULL DEFAULT 0,
                last_checked_at TEXT,
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
                ON snapshots(source_kind, source_id, captured_at);
             CREATE TABLE IF NOT EXISTS cash_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                reference TEXT NOT NULL,
                event_type TEXT NOT NULL,
                amount_eur REAL NOT NULL,
                original_amount REAL NOT NULL,
                currency TEXT NOT NULL,
                occurred_at TEXT NOT NULL,
                UNIQUE(reference, event_type, occurred_at, original_amount)
             );
             CREATE INDEX IF NOT EXISTS cash_events_time ON cash_events(occurred_at);
             CREATE TABLE IF NOT EXISTS history_sync (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                next_path TEXT,
                backfill_complete INTEGER NOT NULL DEFAULT 0,
                last_synced_at TEXT
             );",
        )
        .map_err(to_string)?;
    add_column_if_missing(
        connection,
        "ALTER TABLE wallets ADD COLUMN wallet_type TEXT NOT NULL DEFAULT 'address'",
    )?;
    add_column_if_missing(
        connection,
        "ALTER TABLE wallets ADD COLUMN cached_balance REAL NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "ALTER TABLE wallets ADD COLUMN cached_address_count INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(
        connection,
        "ALTER TABLE wallets ADD COLUMN last_checked_at TEXT",
    )?;
    Ok(())
}

fn add_column_if_missing(connection: &Connection, statement: &str) -> Result<(), String> {
    match connection.execute(statement, []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

pub fn history_sync_state(connection: &Connection) -> Result<HistorySyncState, String> {
    connection
        .query_row(
            "SELECT next_path, backfill_complete, last_synced_at FROM history_sync WHERE id = 1",
            [],
            |row| {
                Ok(HistorySyncState {
                    next_path: row.get(0)?,
                    backfill_complete: row.get::<_, i64>(1)? != 0,
                    last_synced_at: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(to_string)
        .map(|state| {
            state.unwrap_or(HistorySyncState {
                next_path: Some("/equity/history/transactions?limit=50".to_string()),
                backfill_complete: false,
                last_synced_at: None,
            })
        })
}

pub fn save_cash_events(
    connection: &Connection,
    events: &[(CashEvent, f64)],
) -> Result<usize, String> {
    let mut inserted = 0;
    for (event, rate) in events {
        inserted += connection
            .execute(
                "INSERT OR IGNORE INTO cash_events(
                    reference, event_type, amount_eur, original_amount, currency, occurred_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    event.reference,
                    event.event_type,
                    event.amount * rate,
                    event.amount,
                    event.currency,
                    event.date_time,
                ],
            )
            .map_err(to_string)?;
    }
    Ok(inserted)
}

pub fn save_history_sync_state(
    connection: &Connection,
    next_path: Option<&str>,
    backfill_complete: bool,
) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO history_sync(id, next_path, backfill_complete, last_synced_at)
             VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                next_path = excluded.next_path,
                backfill_complete = excluded.backfill_complete,
                last_synced_at = excluded.last_synced_at",
            params![
                next_path,
                i64::from(backfill_complete),
                Utc::now().to_rfc3339(),
            ],
        )
        .map_err(to_string)?;
    Ok(())
}

#[cfg(test)]
pub fn cash_event_count(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("SELECT COUNT(*) FROM cash_events", [], |row| row.get(0))
        .map_err(to_string)
}

pub fn cash_history_summary(connection: &Connection) -> Result<CashHistorySummary, String> {
    let state = history_sync_state(connection)?;
    let (event_count, net_contributions): (i64, f64) = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(
                CASE WHEN event_type IN ('DEPOSIT', 'WITHDRAW', 'WITHDRAWAL')
                     THEN amount_eur ELSE 0 END
             ), 0) FROM cash_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(to_string)?;
    Ok(CashHistorySummary {
        net_contributions,
        event_count,
        backfill_complete: state.backfill_complete,
    })
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
            "SELECT id, portfolio_id, network, address, label, wallet_type,
                    cached_balance, cached_address_count, last_checked_at
             FROM wallets WHERE portfolio_id = ?1 ORDER BY created_at, id",
        )
        .map_err(to_string)?;
    let rows = statement
        .query_map([portfolio_id], |row| {
            let network: String = row.get(2)?;
            let address: String = row.get(3)?;
            let wallet_type: String = row.get(5)?;
            let display_address = if wallet_type == "xpub" {
                "Extended public key".to_string()
            } else {
                address.clone()
            };
            Ok(Wallet {
                id: row.get(0)?,
                portfolio_id: row.get(1)?,
                symbol: network.to_uppercase(),
                network,
                address,
                display_address,
                label: row.get(4)?,
                wallet_type,
                balance: row.get(6)?,
                address_count: row.get::<_, i64>(7)?.max(0) as usize,
                value: 0.0,
                message: None,
                last_checked_at: row.get(8)?,
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

pub fn ensure_portfolio(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row(
            "SELECT id FROM portfolios ORDER BY created_at, id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_string)?
        .map_or_else(|| create_portfolio(connection, "Crypto"), Ok)
}

pub fn add_wallet(
    connection: &Connection,
    portfolio_id: i64,
    network: &str,
    address: &str,
    label: &str,
    wallet_type: &str,
) -> Result<i64, String> {
    connection
        .execute(
            "INSERT INTO wallets(portfolio_id, network, address, label, wallet_type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                portfolio_id,
                network,
                address.trim(),
                label.trim(),
                wallet_type,
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

pub fn update_wallet_cache(
    connection: &Connection,
    id: i64,
    balance: f64,
    address_count: usize,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE wallets SET cached_balance = ?1, cached_address_count = ?2,
                    last_checked_at = ?3 WHERE id = ?4",
            params![balance, address_count as i64, Utc::now().to_rfc3339(), id],
        )
        .map_err(to_string)?;
    Ok(())
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

pub fn simple_return_since(
    connection: &Connection,
    since: &str,
    current_return: f64,
    current_invested: f64,
) -> Result<(f64, f64), String> {
    let starting_return = connection
        .query_row(
            "SELECT value_eur - invested_eur FROM snapshots
             WHERE source_kind = 'total' AND captured_at >= ?1
             ORDER BY captured_at LIMIT 1",
            [since],
            |row| row.get::<_, f64>(0),
        )
        .optional()
        .map_err(to_string)?
        .unwrap_or(current_return);
    let amount = current_return - starting_return;
    let percent = if current_invested > 0.0 {
        amount / current_invested * 100.0
    } else {
        0.0
    };
    Ok((amount, percent))
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

    use crate::models::CashEvent;

    use super::{
        add_wallet, cash_event_count, create_portfolio, ensure_portfolio, history_sync_state,
        initialize, list_portfolios, save_cash_events, save_history_sync_state, save_snapshot,
        simple_return_since,
    };

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
            "address",
        )
        .expect("wallet");

        let portfolios = list_portfolios(&database).expect("portfolio list");
        assert_eq!(portfolios.len(), 1);
        assert_eq!(portfolios[0].wallets.len(), 1);
        assert_eq!(portfolios[0].wallets[0].label, "Ledger");
        assert_eq!(portfolios[0].wallets[0].wallet_type, "address");
        let serialized = serde_json::to_value(&portfolios[0].wallets[0]).expect("wallet JSON");
        assert!(serialized.get("address").is_none());
        assert_eq!(
            serialized
                .get("displayAddress")
                .and_then(|value| value.as_str()),
            Some("0x0000000000000000000000000000000000000000")
        );
    }

    #[test]
    fn migrates_wallets_created_before_xpub_support() {
        let database = Connection::open_in_memory().expect("in-memory database");
        database
            .execute_batch(
                "CREATE TABLE portfolios (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    name TEXT NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE wallets (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    portfolio_id INTEGER NOT NULL,
                    network TEXT NOT NULL,
                    address TEXT NOT NULL,
                    label TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    UNIQUE(network, address, portfolio_id)
                 );
                 INSERT INTO portfolios(name, created_at) VALUES ('Crypto', '2026-01-01T00:00:00Z');
                 INSERT INTO wallets(portfolio_id, network, address, label, created_at)
                 VALUES (1, 'btc', 'bc1qexample', 'Existing wallet', '2026-01-01T00:00:00Z');",
            )
            .expect("legacy schema");

        initialize(&database).expect("migration");
        let portfolios = list_portfolios(&database).expect("portfolio list");
        let wallet = &portfolios[0].wallets[0];
        assert_eq!(wallet.wallet_type, "address");
        assert_eq!(wallet.balance, 0.0);
        assert_eq!(wallet.address_count, 0);
    }

    #[test]
    fn ensures_exactly_one_initial_portfolio() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let first = ensure_portfolio(&database).expect("initial portfolio");
        let second = ensure_portfolio(&database).expect("existing portfolio");
        assert_eq!(first, second);
        assert_eq!(list_portfolios(&database).unwrap().len(), 1);
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

    #[test]
    fn calculates_simple_period_return_without_cash_flow_timing() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        database
            .execute(
                "INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur, invested_eur)
                 VALUES ('total', 0, '2026-08-01T00:00:00+00:00', 1000, 900)",
                [],
            )
            .expect("snapshot");

        let (amount, percent) =
            simple_return_since(&database, "2026-08-01T00:00:00+00:00", 250.0, 1500.0)
                .expect("period return");
        assert!((amount - 150.0).abs() < f64::EPSILON);
        assert!((percent - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn stores_cash_events_idempotently_and_tracks_backfill() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let event = CashEvent {
            reference: "reference-1".to_string(),
            event_type: "DEPOSIT".to_string(),
            amount: 100.0,
            currency: "EUR".to_string(),
            date_time: "2025-01-02T12:00:00Z".to_string(),
        };

        assert_eq!(
            save_cash_events(&database, &[(event.clone(), 1.0)]).unwrap(),
            1
        );
        assert_eq!(save_cash_events(&database, &[(event, 1.0)]).unwrap(), 0);
        assert_eq!(cash_event_count(&database).unwrap(), 1);

        save_history_sync_state(&database, Some("/next"), false).unwrap();
        let state = history_sync_state(&database).unwrap();
        assert_eq!(state.next_path.as_deref(), Some("/next"));
        assert!(!state.backfill_complete);
        assert!(state.last_synced_at.is_some());
    }
}
