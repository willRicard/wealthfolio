use super::{PriceStats, PurchasePrice};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// Missing FX remains explicit on each point and never enters the aggregates.
pub fn fold_prices(
    mut prices: Vec<PurchasePrice>,
    base: &str,
    mut convert: impl FnMut(Decimal, &str, NaiveDate) -> Option<Decimal>,
) -> Result<(Vec<PurchasePrice>, PriceStats)> {
    prices.sort_by(|a, b| {
        a.date.cmp(&b.date).then_with(|| {
            a.activity_id
                .cmp(&b.activity_id)
                .then_with(|| a.item_id.cmp(&b.item_id))
        })
    });
    let mut stats = PriceStats {
        base_currency: base.into(),
        min_price: None,
        avg_price: None,
        latest_price: None,
        latest_date: None,
        purchase_count: prices.len(),
        unconverted_count: 0,
    };
    let mut total = Decimal::ZERO;
    let mut count = 0u64;
    for price in &mut prices {
        price.base_currency = base.into();
        price.unit_price_base = if price.currency == base {
            Some(price.unit_price)
        } else {
            convert(price.unit_price, &price.currency, price.date.date_naive())
        };
        if let Some(value) = price.unit_price_base {
            total = total
                .checked_add(value)
                .context("Procurement price total overflow")?;
            count += 1;
            stats.min_price = Some(stats.min_price.map_or(value, |min| min.min(value)));
            stats.latest_price = Some(value);
            stats.latest_date = Some(price.date);
        } else {
            stats.unconverted_count += 1;
        }
    }
    if count > 0 {
        stats.avg_price = total.checked_div(Decimal::from(count));
    }
    Ok((prices, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn price(id: &str, day: u32, amount: &str, currency: &str) -> PurchasePrice {
        PurchasePrice {
            item_id: "item".into(),
            activity_id: format!("activity-{id}"),
            date: chrono::NaiveDate::from_ymd_opt(2026, 10, day)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc(),
            shop: Some("Shop".into()),
            quantity: Decimal::ONE,
            unit_price: Decimal::from_str(amount).unwrap(),
            currency: currency.into(),
            unit_price_base: None,
            base_currency: String::new(),
        }
    }

    #[test]
    fn converted_prices_only_and_missing_pair_is_flagged() {
        let (rows, stats) = fold_prices(
            vec![
                price("3", 3, "999", "GBP"),
                price("2", 2, "2", "EUR"),
                price("1", 1, "100", "JPY"),
            ],
            "JPY",
            |amount, currency, _| (currency == "EUR").then(|| amount * Decimal::from(150)),
        )
        .unwrap();
        assert_eq!(stats.min_price, Some(Decimal::from(100)));
        assert_eq!(stats.avg_price, Some(Decimal::from(200)));
        assert_eq!(stats.latest_price, Some(Decimal::from(300)));
        assert_eq!(stats.latest_date, Some(rows[1].date));
        assert_eq!(stats.unconverted_count, 1);
        assert_eq!(stats.purchase_count, 3);
        assert_eq!(rows[2].unit_price_base, None);
        assert_eq!(rows[1].unit_price, Decimal::from(2));
    }

    #[test]
    fn same_currency_never_calls_fx_and_same_day_is_deterministic() {
        let (_, stats) = fold_prices(
            vec![price("b", 1, "3", "JPY"), price("a", 1, "1", "JPY")],
            "JPY",
            |_, _, _| panic!("local price must not use FX"),
        )
        .unwrap();
        assert_eq!(stats.latest_price, Some(Decimal::from(3)));
        assert_eq!(stats.avg_price, Some(Decimal::from(2)));
    }

    #[test]
    fn empty_or_unconverted_prices_have_no_statistics() {
        for rows in [vec![], vec![price("a", 1, "2", "EUR")]] {
            let (_, stats) = fold_prices(rows, "JPY", |_, _, _| None).unwrap();
            assert_eq!(stats.min_price, None);
            assert_eq!(stats.avg_price, None);
            assert_eq!(stats.latest_price, None);
            assert_eq!(stats.latest_date, None);
        }
    }

    #[test]
    fn aggregate_overflow_is_an_error() {
        assert!(fold_prices(
            vec![
                price("a", 1, &Decimal::MAX.to_string(), "JPY"),
                price("b", 2, "1", "JPY")
            ],
            "JPY",
            |_, _, _| None
        )
        .is_err());
    }
}
