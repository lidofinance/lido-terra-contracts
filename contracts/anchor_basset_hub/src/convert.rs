// Copyright 2021 Anchor Protocol. Modified by Lido
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//     http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::contract::{query_total_bluna_issued, query_total_stluna_issued};
use crate::math::decimal_division;
use crate::state::{CONFIG, CURRENT_BATCH, PARAMETERS, STATE};
use cosmwasm_std::{
    attr, to_binary, CosmosMsg, DepsMut, Env, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use std::ops::Mul;

pub fn convert_stluna_bluna(
    deps: DepsMut,
    _env: Env,
    stluna_amount: Uint128,
    sender: String,
) -> StdResult<Response> {
    let conf = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;
    let params = PARAMETERS.load(deps.storage)?;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let stluna_contract = deps.api.addr_humanize(
        &conf
            .stluna_token_contract
            .ok_or_else(|| StdError::generic_err("stluna contract must be registred"))?,
    )?;
    let bluna_contract = deps.api.addr_humanize(
        &conf
            .bluna_token_contract
            .ok_or_else(|| StdError::generic_err("bluna contract must be registred"))?,
    )?;

    let denom_equiv = state.stluna_exchange_rate.mul(stluna_amount);

    let bluna_to_mint = decimal_division(denom_equiv, state.bluna_exchange_rate);
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
    let requested_stluna = current_batch.requested_stluna;

    let total_bluna_supply = query_total_bluna_issued(deps.as_ref())?;
    let total_stluna_supply = query_total_stluna_issued(deps.as_ref())?;
    let mut bluna_mint_amount_with_fee = bluna_to_mint;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_to_mint * recovery_fee;
        let required_peg_fee = (total_bluna_supply + bluna_to_mint + requested_bluna_with_fee)
            - (state.total_bond_bluna_amount + denom_equiv);
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_mint_amount_with_fee = bluna_to_mint.checked_sub(peg_fee)?;
    }

    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        prev_state.total_bond_bluna_amount += denom_equiv;
        prev_state.total_bond_stluna_amount = prev_state.total_bond_stluna_amount.checked_sub(denom_equiv)
            .map_err(|_| {
                StdError::generic_err(format!(
                    "Decrease amount cannot exceed total stluna bond amount: {}. Trying to reduce: {}",
                    prev_state.total_bond_stluna_amount, denom_equiv,
                ))
            })?;
        prev_state.update_bluna_exchange_rate(
            total_bluna_supply + bluna_to_mint,
            requested_bluna_with_fee,
        );
        prev_state
            .update_stluna_exchange_rate(total_stluna_supply .checked_sub(stluna_amount).map_err(|_| {
                StdError::generic_err(format!(
                    "Decrease amount cannot exceed total stluna supply: {}. Trying to reduce: {}",
                    total_stluna_supply, stluna_amount,
                ))
            })?, requested_stluna);
        Ok(prev_state)
    })?;

    let messages: Vec<CosmosMsg> = vec![
        mint_message(
            bluna_contract.to_string(),
            sender.clone(),
            bluna_mint_amount_with_fee,
        )?,
        burn_message(stluna_contract.to_string(), stluna_amount)?,
    ];

    let res = Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "convert_stluna"),
        attr("from", sender),
        attr("bluna_exchange_rate", state.bluna_exchange_rate.to_string()),
        attr(
            "stluna_exchange_rate",
            state.stluna_exchange_rate.to_string(),
        ),
        attr("stluna_amount", stluna_amount),
        attr("bluna_amount", bluna_to_mint),
    ]);
    Ok(res)
}

pub fn convert_bluna_stluna(
    deps: DepsMut,
    _env: Env,
    bluna_amount: Uint128,
    sender: String,
) -> StdResult<Response> {
    let conf = CONFIG.load(deps.storage)?;
    let state = STATE.load(deps.storage)?;

    let stluna_contract = deps.api.addr_humanize(
        &conf
            .stluna_token_contract
            .ok_or_else(|| StdError::generic_err("stluna contract must be registred"))?,
    )?;
    let bluna_contract = deps.api.addr_humanize(
        &conf
            .bluna_token_contract
            .ok_or_else(|| StdError::generic_err("bluna contract must be registred"))?,
    )?;

    let params = PARAMETERS.load(deps.storage)?;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
    let requested_stluna_with_fee = current_batch.requested_stluna;

    let total_bluna_supply = query_total_bluna_issued(deps.as_ref())?;
    let total_stluna_supply = query_total_stluna_issued(deps.as_ref())?;

    // Apply peg recovery fee
    let bluna_amount_with_fee: Uint128;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_amount * recovery_fee;
        let required_peg_fee = (total_bluna_supply + current_batch.requested_bluna_with_fee)
            .checked_sub(state.total_bond_bluna_amount)?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_amount_with_fee = bluna_amount.checked_sub(peg_fee)?;
    } else {
        bluna_amount_with_fee = bluna_amount;
    }

    let denom_equiv = state.bluna_exchange_rate.mul(bluna_amount_with_fee);

    let stluna_to_mint = decimal_division(denom_equiv, state.stluna_exchange_rate);

    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        prev_state.total_bond_bluna_amount = prev_state.total_bond_bluna_amount.checked_sub(denom_equiv)
            .map_err(|_| {
                StdError::generic_err(format!(
                    "Decrease amount cannot exceed total bluna bond amount: {}. Trying to reduce: {}",
                    prev_state.total_bond_bluna_amount, denom_equiv,
                ))
            })?;
        prev_state.total_bond_stluna_amount += denom_equiv;
        prev_state.update_bluna_exchange_rate(
            total_bluna_supply.checked_sub(bluna_amount).map_err(|_| {
                StdError::generic_err(format!(
                    "Decrease amount cannot exceed total bluna supply: {}. Trying to reduce: {}",
                    total_bluna_supply, bluna_amount,
                ))
            })?,
            requested_bluna_with_fee,
        );
        prev_state.update_stluna_exchange_rate(
            total_stluna_supply + stluna_to_mint,
            requested_stluna_with_fee,
        );
        Ok(prev_state)
    })?;

    let messages: Vec<CosmosMsg> = vec![
        mint_message(stluna_contract.to_string(), sender.clone(), stluna_to_mint)?,
        burn_message(bluna_contract.to_string(), bluna_amount)?,
    ];

    let res = Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "convert_stluna"),
        attr("from", sender),
        attr("bluna_exchange_rate", state.bluna_exchange_rate.to_string()),
        attr(
            "stluna_exchange_rate",
            state.stluna_exchange_rate.to_string(),
        ),
        attr("bluna_amount", bluna_amount),
        attr("stluna_amount", stluna_to_mint),
    ]);
    Ok(res)
}

fn mint_message(contract: String, recipient: String, amount: Uint128) -> StdResult<CosmosMsg> {
    let mint_msg = Cw20ExecuteMsg::Mint { recipient, amount };
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract,
        msg: to_binary(&mint_msg)?,
        funds: vec![],
    }))
}

fn burn_message(contract: String, amount: Uint128) -> StdResult<CosmosMsg> {
    let burn_msg = Cw20ExecuteMsg::Burn { amount };
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract,
        msg: to_binary(&burn_msg)?,
        funds: vec![],
    }))
}
