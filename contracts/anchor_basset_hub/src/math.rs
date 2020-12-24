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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decimal_division() {
        let a = Uint128(100);
        let b = Decimal::from_ratio(Uint128(10), Uint128(50));
        let res = decimal_division(a, b);
        assert_eq!(res, Uint128(500));
    }

    #[test]
    fn test_decimal_subtraction() {
        let a = Decimal::from_ratio(Uint128(20), Uint128(50));
        let b = Decimal::from_ratio(Uint128(10), Uint128(50));
        let res = decimal_subtraction(a, b);
        assert_eq!(res.to_string(), "0.2");
    }
}
