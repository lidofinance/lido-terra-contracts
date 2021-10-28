use cosmwasm_std::{Coin, Decimal, QuerierWrapper, StdResult, Uint128};

use terra_cosmwasm::TerraQuerier;

static DECIMAL_FRACTION: Uint128 = Uint128::new(1_000_000_000_000_000_000u128);

pub fn compute_tax(querier: &QuerierWrapper, coin: &Coin) -> StdResult<Uint128> {
    // https://docs.terra.money/Reference/Terra-core/Module-specifications/spec-auth.html#stability-fee
    // In addition to the gas fee, the ante handler charges a stability fee that is a percentage of the transaction's value only for the Stable Coins except LUNA.
    if coin.denom == "uluna" {
        return Ok(Uint128::zero());
    }
    let terra_querier = TerraQuerier::new(querier);
    let tax_rate: Decimal = (terra_querier.query_tax_rate()?).rate;
    let tax_cap: Uint128 = (terra_querier.query_tax_cap(coin.denom.to_string())?).cap;
    Ok(std::cmp::min(
        (coin.amount.checked_sub(coin.amount.multiply_ratio(
            DECIMAL_FRACTION,
            DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
        )))?,
        tax_cap,
    ))
}

pub fn deduct_tax(querier: &QuerierWrapper, coin: Coin) -> StdResult<Coin> {
    let tax_amount = compute_tax(querier, &coin)?;
    Ok(Coin {
        denom: coin.denom,
        amount: (coin.amount.checked_sub(tax_amount))?,
    })
}

pub fn compute_lido_fee(amount: Uint128, fee_rate: Decimal) -> StdResult<Uint128> {
    Ok(amount * fee_rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_compute_lido_fee() {
        struct TestCase {
            fee_rate: Decimal,
            value: Uint128,
            expected_fee_value: Uint128,
        }

        let test_cases: Vec<TestCase> = vec![
            TestCase {
                fee_rate: Decimal::from_str("0.05").unwrap(),
                value: Uint128::from(100u128),
                expected_fee_value: Uint128::from(5u128),
            },
            TestCase {
                fee_rate: Decimal::from_str("0.0").unwrap(),
                value: Uint128::from(100u128),
                expected_fee_value: Uint128::from(0u128),
            },
            TestCase {
                fee_rate: Decimal::from_str("0.033").unwrap(),
                value: Uint128::from(500u128),
                expected_fee_value: Uint128::from(16u128),
            },
            TestCase {
                fee_rate: Decimal::from_str("0.04").unwrap(),
                value: Uint128::from(234274526u128),
                expected_fee_value: Uint128::from(9370981u128),
            },
        ];
        for test_case in test_cases {
            let actual_fee = compute_lido_fee(test_case.value, test_case.fee_rate).unwrap();
            assert_eq!(actual_fee, test_case.expected_fee_value);
        }
    }
}
