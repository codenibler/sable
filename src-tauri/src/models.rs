use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub total_value: f64,
    pub invested_value: f64,
    pub cash_value: f64,
    pub total_return: f64,
    pub return_percent: f64,
    pub monthly_return: PeriodReturn,
    pub yearly_return: PeriodReturn,
    pub opessocius_monthly_return: MonthlyWinnings,
    pub net_contributions: f64,
    pub history_event_count: i64,
    pub history_backfill_complete: bool,
    pub currency: String,
    pub updated_at: String,
    pub history: Vec<DataPoint>,
    pub sources: Vec<SourceSummary>,
    pub holdings: Vec<Holding>,
    pub portfolios: Vec<CryptoPortfolio>,
    pub notices: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthlyWinnings {
    pub month: String,
    pub label: String,
    pub amount: f64,
    pub is_override: bool,
    pub default_rate_percent: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodReturn {
    pub amount: f64,
    pub percent: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataPoint {
    pub timestamp: String,
    pub value: f64,
    pub invested: f64,
    #[serde(skip_serializing)]
    pub opessocius_winnings: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummary {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub value: f64,
    pub return_value: f64,
    pub connected: bool,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub id: String,
    pub symbol: String,
    pub name: String,
    pub source: String,
    pub quantity: f64,
    pub price: f64,
    pub value: f64,
    pub return_value: f64,
    pub allocation: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CryptoPortfolio {
    pub id: i64,
    pub name: String,
    pub value: f64,
    pub return_value: f64,
    pub wallets: Vec<Wallet>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Wallet {
    pub id: i64,
    pub portfolio_id: i64,
    pub network: String,
    #[serde(skip_serializing)]
    pub address: String,
    pub display_address: String,
    pub label: String,
    pub wallet_type: String,
    pub address_count: usize,
    pub balance: f64,
    pub symbol: String,
    pub value: f64,
    pub message: Option<String>,
    #[serde(skip_serializing)]
    pub last_checked_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddWalletInput {
    pub portfolio_id: i64,
    pub network: String,
    pub address: String,
    pub label: String,
}

#[derive(Debug)]
pub struct TradingOverview {
    pub total_value: f64,
    pub invested_value: f64,
    pub cash_value: f64,
    pub return_value: f64,
    pub holdings: Vec<Holding>,
}

#[derive(Debug, Clone)]
pub struct CashEvent {
    pub reference: String,
    pub event_type: String,
    pub amount: f64,
    pub currency: String,
    pub date_time: String,
}

#[derive(Debug)]
pub struct TransactionPage {
    pub events: Vec<CashEvent>,
    pub next_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HistorySyncState {
    pub next_path: Option<String>,
    pub backfill_complete: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Debug)]
pub struct CashHistorySummary {
    pub net_contributions: f64,
    pub event_count: i64,
    pub backfill_complete: bool,
}
