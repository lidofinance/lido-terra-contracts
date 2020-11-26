use cosmwasm_std::{Decimal, Uint128};

const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000u128);

/// return a / b
pub fn decimal_division(a: Uint128, b: Decimal) -> Uint128 {
    let decimal = Decimal::from_ratio(a, b * DECIMAL_FRACTIONAL);
    decimal * DECIMAL_FRACTIONAL
}

/// return a - b
pub fn decimal_subtraction(a: Decimal, b: Decimal) -> Decimal {
    let c = (a * DECIMAL_FRACTIONAL - b * DECIMAL_FRACTIONAL).unwrap();
    Decimal::from_ratio(c, Decimal::one() * DECIMAL_FRACTIONAL)
}
