use cosmwasm_std::{Decimal, Uint128};

const DECIMAL_FRACTIONAL: Uint128 = Uint128(1_000_000_000_000u128);

/// return a * b
pub fn decimal_multiplication(a: Decimal, b: Decimal) -> Decimal {
    Decimal::from_ratio(a * DECIMAL_FRACTIONAL * b, DECIMAL_FRACTIONAL)
}

/// return a + b
pub fn decimal_summation(a: Decimal, b: Decimal) -> Decimal {
    let d = a * DECIMAL_FRACTIONAL + b * DECIMAL_FRACTIONAL;
    Decimal::from_ratio(d, Decimal::one() * DECIMAL_FRACTIONAL)
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
    fn test_decimal_multiplication() {
        let a = Uint128(100);
        let b = Decimal::from_ratio(Uint128(1111111), Uint128(10000000));
        let multiplication = decimal_multiplication(Decimal::from_ratio(a, Uint128(1)), b);
        assert_eq!(multiplication.to_string(), "11.11111");
    }

    #[test]
    fn test_decimal_sumation() {
        let a = Decimal::from_ratio(Uint128(20), Uint128(50));
        let b = Decimal::from_ratio(Uint128(10), Uint128(50));
        let res = decimal_summation(a, b);
        assert_eq!(res.to_string(), "0.6");
    }

    #[test]
    fn test_decimal_subtraction() {
        let a = Decimal::from_ratio(Uint128(20), Uint128(50));
        let b = Decimal::from_ratio(Uint128(10), Uint128(50));
        let res = decimal_subtraction(a, b);
        assert_eq!(res.to_string(), "0.2");
    }
}
