// Copyright 2021 Anchor Protocol. Modified by Lido
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::Decimal;

/// return a * b
pub fn decimal_multiplication_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 * a_u256).into();
    c_u256
}

/// return a + b
pub fn decimal_summation_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (b_u256 + a_u256).into();
    c_u256
}

/// return a - b
pub fn decimal_subtraction_in_256(a: Decimal, b: Decimal) -> Decimal {
    let a_u256: Decimal256 = a.into();
    let b_u256: Decimal256 = b.into();
    let c_u256: Decimal = (a_u256 - b_u256).into();
    c_u256
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::{Decimal, Uint128};

    #[test]
    fn test_decimal_multiplication() {
        let a = Uint128::new(100);
        let b = Decimal::from_ratio(Uint128::new(1111111), Uint128::new(10000000));
        let multiplication =
            decimal_multiplication_in_256(Decimal::from_ratio(a, Uint128::new(1)), b);
        assert_eq!(multiplication.to_string(), "11.11111");
    }

    #[test]
    fn test_decimal_sumation() {
        let a = Decimal::from_ratio(Uint128::new(20), Uint128::new(50));
        let b = Decimal::from_ratio(Uint128::new(10), Uint128::new(50));
        let res = decimal_summation_in_256(a, b);
        assert_eq!(res.to_string(), "0.6");
    }

    #[test]
    fn test_decimal_subtraction() {
        let a = Decimal::from_ratio(Uint128::new(20), Uint128::new(50));
        let b = Decimal::from_ratio(Uint128::new(10), Uint128::new(50));
        let res = decimal_subtraction_in_256(a, b);
        assert_eq!(res.to_string(), "0.2");
    }

    #[test]
    fn test_decimal_multiplication_in_256() {
        let a = Uint128::new(100);
        let b = Decimal::from_ratio(Uint128::new(1111111), Uint128::new(10000000));
        let multiplication =
            decimal_multiplication_in_256(Decimal::from_ratio(a, Uint128::new(1)), b);
        assert_eq!(multiplication.to_string(), "11.11111");
    }

    #[test]
    fn test_decimal_sumation_in_256() {
        let a = Decimal::from_ratio(Uint128::new(20), Uint128::new(50));
        let b = Decimal::from_ratio(Uint128::new(10), Uint128::new(50));
        let res = decimal_summation_in_256(a, b);
        assert_eq!(res.to_string(), "0.6");
    }

    #[test]
    fn test_decimal_subtraction_in_256() {
        let a = Decimal::from_ratio(Uint128::new(20), Uint128::new(50));
        let b = Decimal::from_ratio(Uint128::new(10), Uint128::new(50));
        let res = decimal_subtraction_in_256(a, b);
        assert_eq!(res.to_string(), "0.2");
    }
}
