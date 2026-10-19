//! Units an item is measured in. Purchase quantities are expressed in the
//! item's unit, so prices compare as "amount per unit".
pub const UNITS: &[&str] = &[
    "kg", "L", "株", "缶", "個", "束", "玉", "丁", "杯", "箱", "袋", "房", "本", "枚",
];

pub fn is_known_unit(unit: &str) -> bool {
    UNITS.contains(&unit)
}
