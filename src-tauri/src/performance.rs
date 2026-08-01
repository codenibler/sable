use chrono::{DateTime, Utc};

use crate::models::CashFlow;

pub fn money_weighted_return(
    flows: &[CashFlow],
    current_value: f64,
    valuation_time: DateTime<Utc>,
) -> Option<f64> {
    if current_value < 0.0 || flows.is_empty() {
        return None;
    }
    let mut dated_flows = flows
        .iter()
        .filter_map(|flow| {
            flow.occurred_at
                .parse::<DateTime<Utc>>()
                .ok()
                .map(|date| (date, -flow.amount))
        })
        .collect::<Vec<_>>();
    dated_flows.push((valuation_time, current_value));
    if !dated_flows.iter().any(|(_, amount)| *amount < 0.0)
        || !dated_flows.iter().any(|(_, amount)| *amount > 0.0)
    {
        return None;
    }

    let origin = dated_flows.iter().map(|(date, _)| *date).min()?;
    let npv = |rate: f64| {
        dated_flows.iter().fold(0.0, |total, (date, amount)| {
            let years = (*date - origin).num_seconds() as f64 / (365.2425 * 86_400.0);
            total + amount / (1.0 + rate).powf(years)
        })
    };

    let mut low = -0.9999;
    let mut high = 1.0;
    let mut low_value = npv(low);
    let mut high_value = npv(high);
    while low_value.signum() == high_value.signum() && high < 1_000_000.0 {
        high *= 10.0;
        high_value = npv(high);
    }
    if !low_value.is_finite()
        || !high_value.is_finite()
        || low_value.signum() == high_value.signum()
    {
        return None;
    }

    for _ in 0..200 {
        let middle = (low + high) / 2.0;
        let middle_value = npv(middle);
        if middle_value.abs() < 0.000_001 {
            return Some(middle * 100.0);
        }
        if middle_value.signum() == low_value.signum() {
            low = middle;
            low_value = middle_value;
        } else {
            high = middle;
        }
    }
    Some(((low + high) / 2.0) * 100.0)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::money_weighted_return;
    use crate::models::CashFlow;

    #[test]
    fn calculates_annualized_money_weighted_return() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let result = money_weighted_return(
            &[CashFlow {
                occurred_at: start.to_rfc3339(),
                amount: 1_000.0,
            }],
            1_100.0,
            end,
        )
        .unwrap();
        assert!((result - 10.0).abs() < 0.05);
    }

    #[test]
    fn requires_both_investment_and_value() {
        assert!(money_weighted_return(&[], 100.0, Utc::now()).is_none());
    }
}
