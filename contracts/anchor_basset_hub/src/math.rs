use cosmwasm_std::{Decimal, Uint128};

const DECIMAL_FRACTIONAL: u128 = 1_000_000_000_000_000_000u128;

/// return a / b
pub fn decimal_division(a: Uint128, b: Decimal) -> Uint128 {
    let decimal = Decimal::from_ratio(a, b * Uint128::from(DECIMAL_FRACTIONAL));
    decimal * Uint128::from(DECIMAL_FRACTIONAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_division() {
        let a = Uint128::from(100u64);
        let b = Decimal::from_ratio(Uint128::from(10u64), Uint128::from(50u64));
        let res = decimal_division(a, b);
        assert_eq!(res, Uint128::from(500u64));
    }
}
