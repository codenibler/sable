use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{Duration, Utc};
use rusqlite::{Connection, MAIN_DB, OptionalExtension, params};

use crate::models::{
    CashEvent, CashHistorySummary, CryptoPortfolio, DataPoint, HistorySyncState, NetWorthEntry,
    SaveNetWorthInput, Wallet,
};

pub fn open(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(to_string)?;
    initialize(&connection)?;
    Ok(connection)
}

pub fn backup_database(
    connection: &Connection,
    directory: &Path,
    retain: usize,
) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let timestamp = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_millis() * 1_000_000);
    let backup_path = directory.join(format!("sable-backup-{timestamp}.sqlite3"));
    connection
        .backup(MAIN_DB, &backup_path, None)
        .map_err(to_string)?;

    let mut backups = fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("sable-backup-") && name.ends_with(".sqlite3"))
        })
        .collect::<Vec<_>>();
    backups.sort();
    let remove_count = backups.len().saturating_sub(retain);
    for obsolete in backups.into_iter().take(remove_count) {
        fs::remove_file(obsolete).map_err(|error| error.to_string())?;
    }
    Ok(backup_path)
}

pub(crate) fn initialize(connection: &Connection) -> Result<(), String> {
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
                invested_eur REAL NOT NULL,
                opessocius_winnings_eur REAL NOT NULL DEFAULT 0,
                quantity REAL
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
             );
             CREATE TABLE IF NOT EXISTS manual_monthly_winnings (
                source TEXT NOT NULL,
                month_start TEXT NOT NULL,
                amount_eur REAL NOT NULL,
                is_override INTEGER NOT NULL DEFAULT 1,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(source, month_start)
             );
             CREATE TABLE IF NOT EXISTS net_worth_entries (
                date TEXT PRIMARY KEY,
                trading212_eur REAL NOT NULL,
                opessocius_eur REAL NOT NULL,
                okx_eur REAL NOT NULL DEFAULT 0,
                trezor_eur REAL NOT NULL DEFAULT 0,
                bunq_eur REAL NOT NULL DEFAULT 0,
                t212_spending_eur REAL NOT NULL DEFAULT 0,
                ing_eur REAL NOT NULL DEFAULT 0,
                joint_account_eur REAL NOT NULL DEFAULT 0,
                receivables_eur REAL NOT NULL,
                cash_eur REAL NOT NULL,
                misc_eur REAL NOT NULL,
                savings_eur REAL NOT NULL DEFAULT 0,
                spending_eur REAL NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS net_worth_seed_dates (
                date TEXT PRIMARY KEY,
                imported_at TEXT NOT NULL
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
    add_column_if_missing(
        connection,
        "ALTER TABLE snapshots ADD COLUMN opessocius_winnings_eur REAL NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(connection, "ALTER TABLE snapshots ADD COLUMN quantity REAL")?;
    add_column_if_missing(
        connection,
        "ALTER TABLE manual_monthly_winnings ADD COLUMN is_override INTEGER NOT NULL DEFAULT 1",
    )?;
    // Stocks were always held at Trading 212, so every recorded balance moves across as-is.
    rename_column_if_present(
        connection,
        "net_worth_entries",
        "stocks_eur",
        "trading212_eur",
    )?;
    for column in [
        "okx_eur",
        "trezor_eur",
        "bunq_eur",
        "t212_spending_eur",
        "ing_eur",
        "joint_account_eur",
    ] {
        add_column_if_missing(
            connection,
            &format!("ALTER TABLE net_worth_entries ADD COLUMN {column} REAL NOT NULL DEFAULT 0"),
        )?;
    }
    // The crypto category was retired while every recorded snapshot still held a zero there, so
    // no balance is lost. The column is declared NOT NULL without a default in databases created
    // before the retirement, so it has to go rather than linger: an insert that omits it fails.
    drop_column_if_present(connection, "net_worth_entries", "crypto_eur")?;
    Ok(())
}

pub fn import_net_worth_history(
    connection: &Connection,
    entries: &[SaveNetWorthInput],
) -> Result<usize, String> {
    let mut inserted = 0;
    for entry in entries {
        let now = Utc::now().to_rfc3339();
        let was_seeded = connection
            .query_row(
                "SELECT 1 FROM net_worth_seed_dates WHERE date = ?1",
                [&entry.date],
                |_| Ok(()),
            )
            .optional()
            .map_err(to_string)?
            .is_some();
        if was_seeded {
            continue;
        }
        inserted += connection
            .execute(
                "INSERT OR IGNORE INTO net_worth_entries(
                    date, trading212_eur, opessocius_eur, okx_eur, trezor_eur,
                    bunq_eur, t212_spending_eur, ing_eur, joint_account_eur, receivables_eur,
                    cash_eur, misc_eur, savings_eur, spending_eur, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
                params![
                    entry.date,
                    entry.trading212,
                    entry.opessocius,
                    entry.okx,
                    entry.trezor,
                    entry.bunq,
                    entry.t212_spending,
                    entry.ing,
                    entry.joint_account,
                    entry.receivables,
                    entry.cash,
                    entry.misc,
                    entry.savings,
                    entry.spending,
                    now,
                ],
            )
            .map_err(to_string)?;
        connection
            .execute(
                "INSERT OR IGNORE INTO net_worth_seed_dates(date, imported_at) VALUES (?1, ?2)",
                params![entry.date, now],
            )
            .map_err(to_string)?;
    }
    Ok(inserted)
}

pub fn save_net_worth_entry(
    connection: &Connection,
    entry: &SaveNetWorthInput,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO net_worth_entries(
                date, trading212_eur, opessocius_eur, okx_eur, trezor_eur,
                bunq_eur, t212_spending_eur, ing_eur, joint_account_eur, receivables_eur,
                cash_eur, misc_eur, savings_eur, spending_eur, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
             ON CONFLICT(date) DO UPDATE SET
                trading212_eur = excluded.trading212_eur,
                opessocius_eur = excluded.opessocius_eur,
                okx_eur = excluded.okx_eur,
                trezor_eur = excluded.trezor_eur,
                bunq_eur = excluded.bunq_eur,
                t212_spending_eur = excluded.t212_spending_eur,
                ing_eur = excluded.ing_eur,
                joint_account_eur = excluded.joint_account_eur,
                receivables_eur = excluded.receivables_eur,
                cash_eur = excluded.cash_eur,
                misc_eur = excluded.misc_eur,
                savings_eur = excluded.savings_eur,
                spending_eur = excluded.spending_eur,
                updated_at = excluded.updated_at",
            params![
                entry.date,
                entry.trading212,
                entry.opessocius,
                entry.okx,
                entry.trezor,
                entry.bunq,
                entry.t212_spending,
                entry.ing,
                entry.joint_account,
                entry.receivables,
                entry.cash,
                entry.misc,
                entry.savings,
                entry.spending,
                now,
            ],
        )
        .map_err(to_string)?;
    Ok(())
}

pub fn list_net_worth_entries(connection: &Connection) -> Result<Vec<NetWorthEntry>, String> {
    let mut statement = connection
        .prepare(
            "SELECT date, trading212_eur, opessocius_eur, okx_eur, trezor_eur,
                    bunq_eur, t212_spending_eur, ing_eur, joint_account_eur, receivables_eur,
                    cash_eur, misc_eur, savings_eur, spending_eur
             FROM net_worth_entries ORDER BY date",
        )
        .map_err(to_string)?;
    statement
        .query_map([], |row| {
            let entry = SaveNetWorthInput {
                date: row.get(0)?,
                trading212: row.get(1)?,
                opessocius: row.get(2)?,
                okx: row.get(3)?,
                trezor: row.get(4)?,
                bunq: row.get(5)?,
                t212_spending: row.get(6)?,
                ing: row.get(7)?,
                joint_account: row.get(8)?,
                receivables: row.get(9)?,
                cash: row.get(10)?,
                misc: row.get(11)?,
                savings: row.get(12)?,
                spending: row.get(13)?,
            };
            Ok(NetWorthEntry {
                net_worth: entry.net_worth(),
                date: entry.date,
                trading212: entry.trading212,
                opessocius: entry.opessocius,
                okx: entry.okx,
                trezor: entry.trezor,
                bunq: entry.bunq,
                t212_spending: entry.t212_spending,
                ing: entry.ing,
                joint_account: entry.joint_account,
                receivables: entry.receivables,
                cash: entry.cash,
                misc: entry.misc,
                savings: entry.savings,
                spending: entry.spending,
            })
        })
        .map_err(to_string)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_string)
}

pub fn remove_net_worth_entry(connection: &Connection, date: &str) -> Result<(), String> {
    connection
        .execute("DELETE FROM net_worth_entries WHERE date = ?1", [date])
        .map_err(to_string)?;
    Ok(())
}

fn add_column_if_missing(connection: &Connection, statement: &str) -> Result<(), String> {
    match connection.execute(statement, []) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("duplicate column name") => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn rename_column_if_present(
    connection: &Connection,
    table: &str,
    from: &str,
    to: &str,
) -> Result<(), String> {
    let has_old_column = connection
        .query_row(
            "SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, from],
            |_| Ok(()),
        )
        .optional()
        .map_err(to_string)?
        .is_some();
    if !has_old_column {
        return Ok(());
    }
    connection
        .execute(
            &format!("ALTER TABLE {table} RENAME COLUMN {from} TO {to}"),
            [],
        )
        .map_err(to_string)?;
    Ok(())
}

fn drop_column_if_present(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<(), String> {
    let is_present = connection
        .query_row(
            "SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |_| Ok(()),
        )
        .optional()
        .map_err(to_string)?
        .is_some();
    if !is_present {
        return Ok(());
    }
    connection
        .execute(&format!("ALTER TABLE {table} DROP COLUMN {column}"), [])
        .map_err(to_string)?;
    Ok(())
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

pub fn cash_transactions(connection: &Connection) -> Result<Vec<(String, f64)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT occurred_at, SUM(amount_eur) FROM cash_events
             WHERE event_type IN ('DEPOSIT', 'WITHDRAW', 'WITHDRAWAL')
             GROUP BY occurred_at
             HAVING SUM(amount_eur) != 0
             ORDER BY occurred_at",
        )
        .map_err(to_string)?;
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(to_string)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_string)
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
            assets: Vec::new(),
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
                assets: Vec::new(),
                message: None,
                error: None,
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
        .execute(
            "UPDATE portfolios SET name = 'Trezor Safe' WHERE name = 'Crypto'",
            [],
        )
        .map_err(to_string)?;
    connection
        .query_row(
            "SELECT id FROM portfolios ORDER BY created_at, id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(to_string)?
        .map_or_else(|| create_portfolio(connection, "Trezor Safe"), Ok)
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

pub fn update_wallet_metadata(
    connection: &Connection,
    portfolio_id: i64,
    network: &str,
    address: &str,
    label: &str,
    wallet_type: &str,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE wallets SET label = ?1, wallet_type = ?2
             WHERE portfolio_id = ?3 AND network = ?4 AND lower(address) = lower(?5)",
            params![label, wallet_type, portfolio_id, network, address.trim()],
        )
        .map_err(to_string)?;
    Ok(())
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

/// What a source reported this round. A failed request and an account that was emptied on
/// purpose both come out of the providers as a zero, and a partial crypto outage comes out as
/// a plausible smaller number, so the value can never say which happened. Callers know, and
/// have to say: only `Reported` numbers are written, and `Unavailable` keeps the row that is
/// already stored.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// The provider answered. These numbers are stored as they are, a zero included.
    Reported { value: f64, invested: f64 },
    /// The provider could not be read, so it reported nothing at all.
    Unavailable,
}

/// What a snapshot write actually put in the row, which is the previous reading whenever a
/// source was unavailable. Aggregates such as the `total` row are summed in the caller, so
/// they have to be built from these effective numbers rather than from what the providers
/// nominally returned, or a carried-forward component never reaches them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StoredSnapshot {
    pub value: f64,
    pub invested: f64,
}

impl StoredSnapshot {
    /// What a source with nothing behind it contributes to an aggregate.
    pub const EMPTY: Self = Self {
        value: 0.0,
        invested: 0.0,
    };
}

struct LatestSnapshot {
    id: i64,
    captured_at: String,
    value: f64,
    invested: f64,
    opessocius_winnings: f64,
    quantity: Option<f64>,
}

pub fn save_snapshot(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
    reading: Reading,
    opessocius_winnings: f64,
    interval_minutes: i64,
) -> Result<StoredSnapshot, String> {
    save_snapshot_with_quantity(
        connection,
        source_kind,
        source_id,
        reading,
        opessocius_winnings,
        None,
        interval_minutes,
    )
}

pub fn save_crypto_snapshot(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
    reading: Reading,
    quantity: f64,
    interval_minutes: i64,
) -> Result<StoredSnapshot, String> {
    save_snapshot_with_quantity(
        connection,
        source_kind,
        source_id,
        reading,
        0.0,
        Some(quantity),
        interval_minutes,
    )
}

/// What an unavailable source contributes to an aggregate without writing a row of its own,
/// for sources that are skipped entirely while their provider is failing.
pub fn carried_forward_snapshot(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<StoredSnapshot, String> {
    Ok(latest_snapshot(connection, source_kind, source_id)?.map_or(
        StoredSnapshot::EMPTY,
        |previous| StoredSnapshot {
            value: previous.value,
            invested: previous.invested,
        },
    ))
}

fn save_snapshot_with_quantity(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
    reading: Reading,
    opessocius_winnings: f64,
    quantity: Option<f64>,
    interval_minutes: i64,
) -> Result<StoredSnapshot, String> {
    let last = latest_snapshot(connection, source_kind, source_id)?;

    // An unavailable source repeats its previous row whole. The invested basis and the unit
    // count come along with the value: leaving them at zero would make the next real reading
    // look like the entire position had just been transferred in, permanently shifting the
    // basis that returns are measured against.
    let (value, invested, opessocius_winnings, quantity) = match (reading, last.as_ref()) {
        (Reading::Reported { value, invested }, _) => {
            (value, invested, opessocius_winnings, quantity)
        }
        (Reading::Unavailable, Some(previous)) => (
            previous.value,
            previous.invested,
            previous.opessocius_winnings,
            previous.quantity,
        ),
        // Nothing was reported and nothing is stored, so there is nothing to say yet.
        (Reading::Unavailable, None) => (0.0, 0.0, 0.0, quantity.map(|_| 0.0)),
    };

    let recent_id = last.and_then(|previous| {
        previous
            .captured_at
            .parse::<chrono::DateTime<Utc>>()
            .ok()
            .filter(|time| Utc::now() - *time < Duration::minutes(interval_minutes))
            .map(|_| previous.id)
    });

    if let Some(id) = recent_id {
        connection
            .execute(
                "UPDATE snapshots SET captured_at = ?1, value_eur = ?2, invested_eur = ?3,
                        opessocius_winnings_eur = ?4, quantity = ?5 WHERE id = ?6",
                params![
                    Utc::now().to_rfc3339(),
                    value,
                    invested,
                    opessocius_winnings,
                    quantity,
                    id
                ],
            )
            .map_err(to_string)?;
    } else {
        connection
            .execute(
                "INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur, invested_eur,
                        opessocius_winnings_eur, quantity) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    source_kind,
                    source_id,
                    Utc::now().to_rfc3339(),
                    value,
                    invested,
                    opessocius_winnings,
                    quantity
                ],
            )
            .map_err(to_string)?;
    }
    Ok(StoredSnapshot { value, invested })
}

/// The row a fresh reading is measured against: the last one stored, which in the
/// within-interval case is the very row the write is about to overwrite.
fn latest_snapshot(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<Option<LatestSnapshot>, String> {
    connection
        .query_row(
            "SELECT id, captured_at, value_eur, invested_eur, opessocius_winnings_eur, quantity
             FROM snapshots WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY captured_at DESC, id DESC LIMIT 1",
            params![source_kind, source_id],
            |row| {
                Ok(LatestSnapshot {
                    id: row.get(0)?,
                    captured_at: row.get(1)?,
                    value: row.get(2)?,
                    invested: row.get(3)?,
                    opessocius_winnings: row.get(4)?,
                    quantity: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(to_string)
}

pub fn crypto_position(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<Option<(f64, Option<f64>)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT invested_eur, quantity, value_eur FROM snapshots
             WHERE source_kind = ?1 AND source_id = ?2
             ORDER BY captured_at DESC LIMIT 2",
        )
        .map_err(to_string)?;
    let positions = statement
        .query_map(params![source_kind, source_id], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, Option<f64>>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(to_string)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_string)?;
    let Some((mut invested, quantity, latest_value)) = positions.first().copied() else {
        return Ok(None);
    };

    // Before quantity-aware snapshots existed, a freshly received deposit could already
    // have been recorded as a sharp value jump. Repair that most recent legacy point once;
    // subsequent transfers use exact unit deltas below rather than this migration heuristic.
    if quantity.is_none()
        && let Some((_, _, previous_value)) = positions.get(1)
    {
        let increase = latest_value - previous_value;
        if increase > 1.0 && increase > previous_value.abs() * 0.05 {
            invested += increase;
        }
    }
    Ok(Some((invested, quantity)))
}

pub fn total_history(connection: &Connection) -> Result<Vec<DataPoint>, String> {
    source_history(connection, "total", 0)
}

pub fn source_history(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<Vec<DataPoint>, String> {
    let mut statement = connection
        .prepare(
            "SELECT captured_at, value_eur, invested_eur, opessocius_winnings_eur FROM snapshots
             WHERE source_kind = ?1 AND source_id = ?2 ORDER BY captured_at DESC LIMIT 500",
        )
        .map_err(to_string)?;
    let mut points = statement
        .query_map(params![source_kind, source_id], |row| {
            Ok(DataPoint {
                timestamp: row.get(0)?,
                value: row.get(1)?,
                invested: row.get(2)?,
                opessocius_winnings: row.get(3)?,
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
    current_opessocius_winnings: f64,
    distributed_opessocius_return: f64,
) -> Result<(f64, f64), String> {
    let starting_return = connection
        .query_row(
            "SELECT value_eur - invested_eur, opessocius_winnings_eur FROM snapshots
             WHERE source_kind = 'total' AND captured_at >= ?1
             ORDER BY captured_at LIMIT 1",
            [since],
            |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
        )
        .optional()
        .map_err(to_string)?
        .unwrap_or((current_return, current_opessocius_winnings));
    let amount = (current_return - current_opessocius_winnings)
        - (starting_return.0 - starting_return.1)
        + distributed_opessocius_return;
    let percent = if current_invested > 0.0 {
        amount / current_invested * 100.0
    } else {
        0.0
    };
    Ok((amount, percent))
}

pub fn save_monthly_winnings(
    connection: &Connection,
    source: &str,
    month_start: &str,
    amount: f64,
    is_override: bool,
) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO manual_monthly_winnings(
                source, month_start, amount_eur, is_override, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(source, month_start) DO UPDATE SET
                amount_eur = excluded.amount_eur,
                is_override = excluded.is_override,
                updated_at = excluded.updated_at",
            params![source, month_start, amount, i64::from(is_override), now],
        )
        .map_err(to_string)?;
    Ok(())
}

pub fn monthly_winning(
    connection: &Connection,
    source: &str,
    month_start: &str,
) -> Result<Option<(f64, bool)>, String> {
    connection
        .query_row(
            "SELECT amount_eur, is_override FROM manual_monthly_winnings
             WHERE source = ?1 AND month_start = ?2",
            params![source, month_start],
            |row| Ok((row.get(0)?, row.get::<_, i64>(1)? != 0)),
        )
        .optional()
        .map_err(to_string)
}

pub fn monthly_winnings(
    connection: &Connection,
    source: &str,
) -> Result<Vec<(String, f64)>, String> {
    let mut statement = connection
        .prepare(
            "SELECT month_start, amount_eur FROM manual_monthly_winnings
             WHERE source = ?1 ORDER BY month_start",
        )
        .map_err(to_string)?;
    statement
        .query_map([source], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(to_string)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_string)
}

#[cfg(test)]
pub fn snapshot_baseline(
    connection: &Connection,
    source_kind: &str,
    source_id: i64,
) -> Result<Option<f64>, String> {
    connection
        .query_row(
            "SELECT invested_eur FROM snapshots WHERE source_kind = ?1 AND source_id = ?2
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

    use crate::models::{CashEvent, SaveNetWorthInput};

    use super::{
        Reading, StoredSnapshot, add_wallet, carried_forward_snapshot, cash_event_count,
        create_portfolio, crypto_position, ensure_portfolio, history_sync_state,
        import_net_worth_history, initialize, list_net_worth_entries, list_portfolios,
        monthly_winning, monthly_winnings, remove_net_worth_entry, save_cash_events,
        save_crypto_snapshot, save_history_sync_state, save_monthly_winnings, save_net_worth_entry,
        save_snapshot, simple_return_since, snapshot_baseline, source_history,
        update_wallet_metadata,
    };

    /// The stored values for a source, oldest first. Snapshots taken with an interval of zero
    /// minutes always insert, so a test can lay down a series of rounds without waiting.
    fn stored_values(connection: &Connection, source_kind: &str, source_id: i64) -> Vec<f64> {
        source_history(connection, source_kind, source_id)
            .expect("stored history")
            .iter()
            .map(|point| point.value)
            .collect()
    }

    fn reported(value: f64, invested: f64) -> Reading {
        Reading::Reported { value, invested }
    }

    #[test]
    fn imports_and_updates_net_worth_entries_by_date() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let mut entry = SaveNetWorthInput {
            date: "2026-07-27".to_string(),
            trading212: 10_919.67,
            opessocius: 8_028.04,
            okx: 0.0,
            trezor: 0.0,
            bunq: 0.0,
            t212_spending: 0.0,
            ing: 0.0,
            joint_account: 0.0,
            receivables: 1_033.75,
            cash: 40.0,
            misc: 0.0,
            savings: 125.0,
            spending: 217.85,
        };
        assert_eq!(
            import_net_worth_history(&database, &[entry.clone()]).unwrap(),
            1
        );
        assert_eq!(
            import_net_worth_history(&database, &[entry.clone()]).unwrap(),
            0
        );

        entry.misc = 100.0;
        save_net_worth_entry(&database, &entry).unwrap();
        let entries = list_net_worth_entries(&database).unwrap();
        assert_eq!(entries.len(), 1);
        assert!((entries[0].net_worth - 20_464.31).abs() < 0.001);
        assert_eq!(entries[0].misc, 100.0);
        remove_net_worth_entry(&database, "2026-07-27").unwrap();
        assert!(list_net_worth_entries(&database).unwrap().is_empty());
        assert_eq!(import_net_worth_history(&database, &[entry]).unwrap(), 0);
        assert!(list_net_worth_entries(&database).unwrap().is_empty());
    }

    #[test]
    fn migrates_stock_balances_recorded_before_the_trading212_rename() {
        let database = Connection::open_in_memory().expect("in-memory database");
        database
            .execute_batch(
                "CREATE TABLE net_worth_entries (
                    date TEXT PRIMARY KEY,
                    stocks_eur REAL NOT NULL,
                    opessocius_eur REAL NOT NULL,
                    crypto_eur REAL NOT NULL,
                    savings_eur REAL NOT NULL,
                    spending_eur REAL NOT NULL,
                    receivables_eur REAL NOT NULL,
                    cash_eur REAL NOT NULL,
                    misc_eur REAL NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );
                 INSERT INTO net_worth_entries VALUES (
                    '2026-07-27', 10919.67, 8028.04, 0, 125, 217.85, 1033.75, 40, 0,
                    '2026-07-27T00:00:00Z', '2026-07-27T00:00:00Z'
                 );",
            )
            .expect("legacy schema");

        initialize(&database).expect("migration");
        let entries = list_net_worth_entries(&database).expect("entries");
        assert_eq!(entries[0].trading212, 10_919.67);
        assert_eq!(entries[0].savings, 125.0);
        assert_eq!(entries[0].okx, 0.0);
        assert_eq!(entries[0].joint_account, 0.0);
        assert!((entries[0].net_worth - 20_364.31).abs() < 0.001);

        // The retired crypto column was NOT NULL without a default, so it has to be gone for a
        // save that no longer supplies it to succeed on a database created before the removal.
        let entry = SaveNetWorthInput {
            date: "2026-07-28".to_string(),
            trading212: 11_000.0,
            opessocius: 8_028.04,
            okx: 0.0,
            trezor: 0.0,
            bunq: 0.0,
            t212_spending: 0.0,
            ing: 0.0,
            joint_account: 0.0,
            receivables: 1_033.75,
            cash: 40.0,
            misc: 0.0,
            savings: 125.0,
            spending: 217.85,
        };
        save_net_worth_entry(&database, &entry).expect("save after the crypto column is dropped");
        assert_eq!(list_net_worth_entries(&database).expect("entries").len(), 2);
    }

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
    fn upgrades_a_configured_wallet_to_an_everstake_position() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let portfolio_id = create_portfolio(&database, "Cold storage").expect("portfolio");
        let pool = "0x0000000000000000000000000000000000000001";
        add_wallet(
            &database,
            portfolio_id,
            "eth",
            pool,
            "Staked Ethereum wallet",
            "address",
        )
        .expect("legacy configured wallet");

        update_wallet_metadata(
            &database,
            portfolio_id,
            "eth",
            pool,
            "Everstake staked ETH",
            "everstake",
        )
        .expect("metadata update");

        let portfolios = list_portfolios(&database).expect("portfolio list");
        assert_eq!(portfolios[0].wallets[0].label, "Everstake staked ETH");
        assert_eq!(portfolios[0].wallets[0].wallet_type, "everstake");
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
        let portfolios = list_portfolios(&database).unwrap();
        assert_eq!(portfolios.len(), 1);
        assert_eq!(portfolios[0].name, "Trezor Safe");
    }

    #[test]
    fn refreshes_the_current_snapshot_bucket() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "total", 0, reported(10.0, 8.0), 0.0, 60).expect("first snapshot");
        save_snapshot(&database, "total", 0, reported(12.0, 8.0), 0.0, 60)
            .expect("updated snapshot");

        let (count, value): (i64, f64) = database
            .query_row(
                "SELECT COUNT(*), MAX(value_eur) FROM snapshots WHERE source_kind = 'total'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("snapshot query");
        assert_eq!(count, 1);
        assert_eq!(value, 12.0);
        assert_eq!(snapshot_baseline(&database, "total", 0).unwrap(), Some(8.0));
    }

    #[test]
    fn carries_a_known_balance_forward_when_a_source_cannot_be_read() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(740.69, 500.0), 0.0, 0)
            .expect("healthy round");

        let stored = save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 0)
            .expect("failed round");

        assert_eq!(
            stored,
            StoredSnapshot {
                value: 740.69,
                invested: 500.0
            }
        );
        assert_eq!(stored_values(&database, "portfolio", 1), [740.69, 740.69]);
    }

    /// A provider can stay down for many rounds, and every one of them has to repeat the last
    /// real reading rather than let the outage compound.
    #[test]
    fn carries_forward_across_consecutive_failed_rounds() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(740.69, 500.0), 0.0, 0)
            .expect("healthy round");
        for round in 0..4 {
            save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 0)
                .unwrap_or_else(|_| panic!("failed round {round}"));
        }

        assert_eq!(
            stored_values(&database, "portfolio", 1),
            [740.69, 740.69, 740.69, 740.69, 740.69]
        );
    }

    /// The carry-forward must not become permanent: a provider that answers with a zero has
    /// emptied the account, and that zero is a real reading however long the outage before it.
    #[test]
    fn stores_a_zero_from_a_source_that_reports_one_after_an_outage() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(740.69, 500.0), 0.0, 0)
            .expect("healthy round");
        save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 0)
            .expect("failed round");

        let stored = save_snapshot(&database, "portfolio", 1, reported(0.0, 0.0), 0.0, 0)
            .expect("emptied round");

        assert_eq!(stored, StoredSnapshot::EMPTY);
        assert_eq!(
            stored_values(&database, "portfolio", 1),
            [740.69, 740.69, 0.0]
        );
    }

    #[test]
    fn stores_a_zero_from_a_source_that_has_no_history_to_carry() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");

        let stored = save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 0)
            .expect("first round");

        assert_eq!(stored, StoredSnapshot::EMPTY);
        assert_eq!(stored_values(&database, "portfolio", 1), [0.0]);
    }

    #[test]
    fn keeps_storing_zero_for_a_source_that_holds_nothing() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(0.0, 0.0), 0.0, 0).expect("first round");
        save_snapshot(&database, "portfolio", 1, reported(0.0, 0.0), 0.0, 0).expect("second round");

        assert_eq!(stored_values(&database, "portfolio", 1), [0.0, 0.0]);
    }

    #[test]
    fn stores_a_fresh_reading_from_a_source_that_answers() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(740.69, 500.0), 0.0, 0)
            .expect("first round");

        let stored = save_snapshot(&database, "portfolio", 1, reported(801.2, 500.0), 0.0, 0)
            .expect("second round");

        assert_eq!(
            stored,
            StoredSnapshot {
                value: 801.2,
                invested: 500.0
            }
        );
        assert_eq!(stored_values(&database, "portfolio", 1), [740.69, 801.2]);
    }

    /// Within the snapshot interval the write overwrites the very row that holds the previous
    /// reading, so the carry-forward has to be read out of it before it is replaced.
    #[test]
    fn carries_forward_while_refreshing_the_current_snapshot_bucket() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_snapshot(&database, "portfolio", 1, reported(740.69, 500.0), 0.0, 60)
            .expect("healthy round");

        let stored = save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 60)
            .expect("failed round");

        assert_eq!(
            stored,
            StoredSnapshot {
                value: 740.69,
                invested: 500.0
            }
        );
        assert_eq!(stored_values(&database, "portfolio", 1), [740.69]);
    }

    /// Reproduces 2026-08-09 17:48, when Trading 212 and the crypto providers both answered
    /// with nothing and the total row recorded the Opessocius balance alone as a cliff.
    #[test]
    fn keeps_the_total_intact_when_its_components_cannot_be_read() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        let opessocius = 8_028.05;

        let trading = save_snapshot(
            &database,
            "trading212",
            0,
            reported(10_906.17, 9_000.0),
            0.0,
            0,
        )
        .expect("healthy broker");
        let crypto = save_snapshot(&database, "portfolio", 1, reported(740.69, 600.0), 0.0, 0)
            .expect("healthy crypto");
        save_snapshot(
            &database,
            "total",
            0,
            reported(
                trading.value + crypto.value + opessocius,
                trading.invested + crypto.invested,
            ),
            0.0,
            0,
        )
        .expect("healthy total");

        // The broker is unreachable, so it is not snapshotted at all, and crypto cannot be read.
        let trading =
            carried_forward_snapshot(&database, "trading212", 0).expect("unreachable broker");
        let crypto = save_snapshot(&database, "portfolio", 1, Reading::Unavailable, 0.0, 0)
            .expect("unreadable crypto");
        let total = save_snapshot(
            &database,
            "total",
            0,
            reported(
                trading.value + crypto.value + opessocius,
                trading.invested + crypto.invested,
            ),
            0.0,
            0,
        )
        .expect("outage total");

        assert!((total.value - 19_674.91).abs() < 0.001);
        let history = stored_values(&database, "total", 0);
        assert_eq!(history.len(), 2);
        assert_eq!(history[1], history[0]);
        assert_eq!(stored_values(&database, "trading212", 0), [10_906.17]);
    }

    /// The carried-forward row stands in for a reading that never arrived, so it has to keep
    /// the units too: a zero quantity would look like the whole position had been transferred
    /// out and would rewrite the invested basis on the next real reading.
    #[test]
    fn keeps_the_crypto_position_while_a_source_cannot_be_read() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_crypto_snapshot(&database, "crypto-eth", 1, reported(182.0, 150.0), 0.05, 0)
            .expect("healthy round");

        save_crypto_snapshot(&database, "crypto-eth", 1, Reading::Unavailable, 0.0, 0)
            .expect("failed round");

        assert_eq!(
            crypto_position(&database, "crypto-eth", 1).unwrap(),
            Some((150.0, Some(0.05)))
        );
    }

    #[test]
    fn stores_crypto_quantity_with_its_invested_basis() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_crypto_snapshot(&database, "crypto-btc", 1, reported(120.0, 100.0), 1.0, 60)
            .expect("crypto snapshot");

        assert_eq!(
            crypto_position(&database, "crypto-btc", 1).unwrap(),
            Some((100.0, Some(1.0)))
        );
    }

    #[test]
    fn repairs_a_sharp_deposit_jump_from_legacy_snapshots() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        database
            .execute_batch(
                "INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur, invested_eur)
                 VALUES ('crypto-btc', 1, '2026-08-04T00:00:00Z', 236, 236);
                 INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur, invested_eur)
                 VALUES ('crypto-btc', 1, '2026-08-05T00:00:00Z', 324, 236);",
            )
            .expect("legacy snapshots");

        assert_eq!(
            crypto_position(&database, "crypto-btc", 1).unwrap(),
            Some((324.0, None))
        );
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

        let (amount, percent) = simple_return_since(
            &database,
            "2026-08-01T00:00:00+00:00",
            250.0,
            1500.0,
            0.0,
            0.0,
        )
        .expect("period return");
        assert!((amount - 150.0).abs() < f64::EPSILON);
        assert!((percent - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn upserts_monthly_winnings_and_replaces_the_recording_jump() {
        let database = Connection::open_in_memory().expect("in-memory database");
        initialize(&database).expect("schema");
        save_monthly_winnings(&database, "opessocius", "2026-07-01", 100.0, false).unwrap();
        save_monthly_winnings(&database, "opessocius", "2026-07-01", 120.0, true).unwrap();
        assert_eq!(
            monthly_winnings(&database, "opessocius").unwrap(),
            vec![("2026-07-01".to_string(), 120.0)]
        );
        assert_eq!(
            monthly_winning(&database, "opessocius", "2026-07-01").unwrap(),
            Some((120.0, true))
        );

        database
            .execute(
                "INSERT INTO snapshots(source_kind, source_id, captured_at, value_eur,
                        invested_eur, opessocius_winnings_eur)
                 VALUES ('total', 0, '2026-01-01T00:00:00+00:00', 1000, 900, 0)",
                [],
            )
            .expect("snapshot");
        let (amount, _) = simple_return_since(
            &database,
            "2026-01-01T00:00:00+00:00",
            220.0,
            1000.0,
            120.0,
            120.0,
        )
        .unwrap();
        assert!((amount - 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn treats_returns_from_the_old_schema_as_manual_overrides() {
        let database = Connection::open_in_memory().expect("in-memory database");
        database
            .execute_batch(
                "CREATE TABLE manual_monthly_winnings (
                    source TEXT NOT NULL,
                    month_start TEXT NOT NULL,
                    amount_eur REAL NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    PRIMARY KEY(source, month_start)
                 );
                 INSERT INTO manual_monthly_winnings VALUES (
                    'opessocius', '2026-07-01', 75, '2026-08-01', '2026-08-01'
                 );",
            )
            .expect("legacy schema");
        initialize(&database).expect("migrated schema");

        assert_eq!(
            monthly_winning(&database, "opessocius", "2026-07-01").unwrap(),
            Some((75.0, true))
        );
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
