use crate::registry::Validator;
use cosmwasm_std::{StdError, StdResult, Uint128};
use std::ops::Sub;

pub fn calculate_delegations(
    mut amoint_to_delegate: Uint128,
    validators: &[Validator],
) -> StdResult<(Uint128, Vec<Uint128>)> {
    let total_delegated: u128 = validators.iter().map(|v| v.total_delegated.0).sum();
    let total_coins_to_distribute = Uint128::from(total_delegated) + amoint_to_delegate;
    let coins_per_validator = total_coins_to_distribute.0 / validators.len() as u128;
    let remaining_coins = total_coins_to_distribute.0 % validators.len() as u128;

    let mut delegations = vec![Uint128(0); validators.len()];
    for (index, validator) in validators.iter().enumerate() {
        let extra_coin = if (index + 1) as u128 <= remaining_coins {
            1u128
        } else {
            0u128
        };
        if coins_per_validator + extra_coin < validator.total_delegated.0 {
            continue;
        }
        let mut to_delegate =
            Uint128::from(coins_per_validator + extra_coin).sub(validator.total_delegated)?;
        if to_delegate > amoint_to_delegate {
            to_delegate = amoint_to_delegate
        }
        delegations[index] = to_delegate;
        amoint_to_delegate = amoint_to_delegate.sub(to_delegate)?;
        if amoint_to_delegate.is_zero() {
            break;
        }
    }
    Ok((amoint_to_delegate, delegations))
}

pub fn calculate_undelegations(
    mut undelegation_amount: Uint128,
    validators: &[Validator],
) -> StdResult<Vec<Uint128>> {
    let total_delegated: u128 = validators.iter().map(|v| v.total_delegated.0).sum();

    if undelegation_amount.0 > total_delegated {
        return Err(StdError::generic_err(
            "undelegate amount can't be bigger than total delegated amount",
        ));
    }

    let total_coins_after_undelegation = Uint128::from(total_delegated).sub(undelegation_amount)?;
    let coins_per_validator = total_coins_after_undelegation.0 / validators.len() as u128;
    let remaining_coins = total_coins_after_undelegation.0 % validators.len() as u128;

    let mut undelegations = vec![Uint128(0); validators.len()];
    for (index, validator) in validators.iter().enumerate() {
        let extra_coin = if (index + 1) as u128 <= remaining_coins {
            1u128
        } else {
            0u128
        };
        let mut to_undelegate = validator
            .total_delegated
            .sub(Uint128::from(coins_per_validator + extra_coin))?;
        if to_undelegate > undelegation_amount {
            to_undelegate = undelegation_amount
        }
        undelegations[index] = to_undelegate;
        undelegation_amount = undelegation_amount.sub(to_undelegate)?;
        if undelegation_amount.is_zero() {
            break;
        }
    }
    Ok(undelegations)
}
