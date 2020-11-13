use cosmwasm_std::{Decimal, Uint128};

const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000u128);

/// return a / b
pub fn decimal_division(a: Uint128, b: Decimal) -> Uint128 {
    let decimal = Decimal::from_ratio(a, b * DECIMAL_FRACTIONAL);
    decimal * DECIMAL_FRACTIONAL
}
