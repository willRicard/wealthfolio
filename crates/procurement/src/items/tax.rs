//! How a shop reports an item's price. Stored receipt subtotals are interpreted
//! using this tax type so procurement prices always compare tax-included.
use rust_decimal::Decimal;

pub const TAX_TYPES: &[&str] = &["INCLUDED", "EXCLUDED_8", "EXCLUDED_10", "EXEMPT"];

pub fn is_known_tax_type(tax_type: &str) -> bool {
    TAX_TYPES.contains(&tax_type)
}

/// Multiplier taking an entered subtotal to its tax-included value.
pub fn tax_multiplier(tax_type: &str) -> Decimal {
    match tax_type {
        "EXCLUDED_8" => Decimal::new(108, 2),
        "EXCLUDED_10" => Decimal::new(110, 2),
        _ => Decimal::ONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_tax_types_and_their_multipliers() {
        for (value, multiplier) in [
            ("INCLUDED", Decimal::ONE),
            ("EXCLUDED_8", Decimal::new(108, 2)),
            ("EXCLUDED_10", Decimal::new(110, 2)),
            ("EXEMPT", Decimal::ONE),
        ] {
            assert!(is_known_tax_type(value));
            assert_eq!(tax_multiplier(value), multiplier);
        }
        for value in ["", "included", "EXCLUDED_5", " INCLUDED "] {
            assert!(!is_known_tax_type(value));
        }
    }
}
