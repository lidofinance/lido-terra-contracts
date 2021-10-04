// Copyright 2021 Lido
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

use crate::registry::Validator;
use cosmwasm_std::{StdError, StdResult, Uint128};
use std::ops::Sub;

pub fn calculate_delegations(
    mut amount_to_delegate: Uint128,
    validators: &[Validator],
) -> StdResult<(Uint128, Vec<Uint128>)> {
    if validators.is_empty() {
        return Err(StdError::generic_err("Empty validators set"));
    }
    let total_delegated: u128 = validators.iter().map(|v| v.total_delegated.u128()).sum();
    let total_coins_to_distribute = Uint128::from(total_delegated) + amount_to_delegate;
    let coins_per_validator = total_coins_to_distribute.u128() / validators.len() as u128;
    let remaining_coins = total_coins_to_distribute.u128() % validators.len() as u128;

    let mut delegations = vec![Uint128::zero(); validators.len()];
    for (index, validator) in validators.iter().enumerate() {
        let extra_coin = if (index + 1) as u128 <= remaining_coins {
            1u128
        } else {
            0u128
        };
        if coins_per_validator + extra_coin < validator.total_delegated.u128() {
            continue;
        }
        let mut to_delegate =
            Uint128::from(coins_per_validator + extra_coin).sub(validator.total_delegated);
        if to_delegate > amount_to_delegate {
            to_delegate = amount_to_delegate
        }
        delegations[index] = to_delegate;
        amount_to_delegate = amount_to_delegate.checked_sub(to_delegate)?;
        if amount_to_delegate.is_zero() {
            break;
        }
    }
    Ok((amount_to_delegate, delegations))
}

pub fn calculate_undelegations(
    mut undelegation_amount: Uint128,
    validators: &[Validator],
) -> StdResult<Vec<Uint128>> {
    if validators.is_empty() {
        return Err(StdError::generic_err("Empty validators set"));
    }

    let total_delegated: u128 = validators.iter().map(|v| v.total_delegated.u128()).sum();

    if undelegation_amount.u128() > total_delegated {
        return Err(StdError::generic_err(
            "undelegate amount can't be bigger than total delegated amount",
        ));
    }

    let total_coins_after_undelegation = Uint128::from(total_delegated).sub(undelegation_amount);
    let coins_per_validator = total_coins_after_undelegation.u128() / validators.len() as u128;
    let remaining_coins = total_coins_after_undelegation.u128() % validators.len() as u128;

    let mut undelegations = vec![Uint128::zero(); validators.len()];
    for (index, validator) in validators.iter().enumerate() {
        let extra_coin = if (index + 1) as u128 <= remaining_coins {
            1u128
        } else {
            0u128
        };
        let mut to_undelegate = validator
            .total_delegated
            .sub(Uint128::from(coins_per_validator + extra_coin));
        if to_undelegate > undelegation_amount {
            to_undelegate = undelegation_amount
        }
        undelegations[index] = to_undelegate;
        undelegation_amount = undelegation_amount.checked_sub(to_undelegate)?;
        if undelegation_amount.is_zero() {
            break;
        }
    }
    Ok(undelegations)
}
