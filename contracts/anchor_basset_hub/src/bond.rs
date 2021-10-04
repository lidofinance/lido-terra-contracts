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

use crate::contract::{query_total_bluna_issued, query_total_stluna_issued, slashing};
use crate::math::decimal_division;
use crate::state::{CONFIG, CURRENT_BATCH, PARAMETERS, STATE};
use anchor_basset_validators_registry::common::calculate_delegations;
use anchor_basset_validators_registry::msg::QueryMsg as QueryValidators;
use anchor_basset_validators_registry::registry::Validator;
use basset::hub::BondType;
use cosmwasm_std::{
    attr, to_binary, Coin, CosmosMsg, DepsMut, Env, MessageInfo, QueryRequest, Response,
    StakingMsg, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

pub fn execute_bond(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    bond_type: BondType,
) -> Result<Response, StdError> {
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;
    let config = CONFIG.load(deps.storage)?;

    let reward_dispatcher_addr =
        deps.api
            .addr_humanize(&config.reward_dispatcher_contract.ok_or_else(|| {
                StdError::generic_err("the reward dispatcher contract must have been registered")
            })?)?;

    if bond_type == BondType::BondRewards && info.sender != reward_dispatcher_addr {
        return Err(StdError::generic_err("unauthorized"));
    }

    // current batch requested fee is need for accurate exchange rate computation.
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_with_fee = match bond_type {
        BondType::BLuna => current_batch.requested_bluna_with_fee,
        BondType::StLuna | BondType::BondRewards => current_batch.requested_stluna,
    };

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if info.funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    // coin must have be sent along with transaction and it should be in underlying coin denom
    let payment = info
        .funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;
    // check slashing
    slashing(&mut deps, env, info.clone())?;

    let state = STATE.load(deps.storage)?;
    let sender = info.sender.clone();

    // get the total supply
    let mut total_supply = match bond_type {
        BondType::BLuna => query_total_bluna_issued(deps.as_ref()).unwrap_or_default(),
        BondType::StLuna | BondType::BondRewards => {
            query_total_stluna_issued(deps.as_ref()).unwrap_or_default()
        }
    };

    // peg recovery fee should be considered
    let mint_amount = match bond_type {
        BondType::BLuna => {
            let bluna_mint_amount = decimal_division(payment.amount, state.bluna_exchange_rate);
            let mut mint_amount_with_fee = bluna_mint_amount;
            if state.bluna_exchange_rate < threshold {
                let max_peg_fee = bluna_mint_amount * recovery_fee;
                let required_peg_fee =
                    (total_supply + bluna_mint_amount + current_batch.requested_bluna_with_fee)
                        - (state.total_bond_bluna_amount + payment.amount);
                let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
                mint_amount_with_fee = bluna_mint_amount.checked_sub(peg_fee)?;
            }
            mint_amount_with_fee
        }
        BondType::StLuna => decimal_division(payment.amount, state.stluna_exchange_rate),
        BondType::BondRewards => Uint128::zero(),
    };

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount;

    // exchange rate should be updated for future
    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        match bond_type {
            BondType::BLuna => {
                prev_state.total_bond_bluna_amount += payment.amount;
                prev_state.update_bluna_exchange_rate(total_supply, requested_with_fee);
                Ok(prev_state)
            }
            BondType::BondRewards => {
                prev_state.total_bond_stluna_amount += payment.amount;
                prev_state.update_stluna_exchange_rate(total_supply, requested_with_fee);
                Ok(prev_state)
            }
            BondType::StLuna => {
                prev_state.total_bond_stluna_amount += payment.amount;
                Ok(prev_state)
            }
        }
    })?;

    let validators_registry_contract = if let Some(v) = config.validators_registry_contract {
        v
    } else {
        return Err(StdError::generic_err(
            "Validators registry contract address is empty",
        ));
    };
    let validators: Vec<Validator> = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: deps
            .api
            .addr_humanize(&validators_registry_contract)?
            .to_string(),
        msg: to_binary(&QueryValidators::GetValidatorsForDelegation {})?,
    }))?;

    if validators.is_empty() {
        return Err(StdError::generic_err("Validators registry is empty"));
    }

    let (_remaining_buffered_balance, delegations) =
        calculate_delegations(payment.amount, validators.as_slice())?;

    let mut external_call_msgs: Vec<cosmwasm_std::CosmosMsg> = vec![];
    for i in 0..delegations.len() {
        if delegations[i].is_zero() {
            continue;
        }
        external_call_msgs.push(cosmwasm_std::CosmosMsg::Staking(StakingMsg::Delegate {
            validator: validators[i].address.clone(),
            amount: Coin::new(delegations[i].u128(), payment.denom.as_str()),
        }));
    }

    //we don't need to mint stLuna when bonding rewards
    if bond_type == BondType::BondRewards {
        let res = Response::new()
            .add_messages(external_call_msgs)
            .add_attributes(vec![
                attr("action", "bond_rewards"),
                attr("from", sender),
                attr("bonded", payment.amount),
            ]);
        return Ok(res);
    }

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender.to_string(),
        amount: mint_amount,
    };

    let token_address = match bond_type {
        BondType::BLuna => deps
            .api
            .addr_humanize(&config.bluna_token_contract.ok_or_else(|| {
                StdError::generic_err("the token contract must have been registered")
            })?)?,
        BondType::StLuna => deps
            .api
            .addr_humanize(&config.stluna_token_contract.ok_or_else(|| {
                StdError::generic_err("the token contract must have been registered")
            })?)?,
        BondType::BondRewards => {
            return Err(StdError::generic_err(
                "can't mint tokens when bonding rewards",
            ));
        }
    };

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address.to_string(),
        msg: to_binary(&mint_msg)?,
        funds: vec![],
    }));

    let res = Response::new()
        .add_messages(external_call_msgs)
        .add_attributes(vec![
            attr("action", "mint"),
            attr("from", sender),
            attr("bonded", payment.amount),
            attr("minted", mint_amount),
        ]);
    Ok(res)
}
