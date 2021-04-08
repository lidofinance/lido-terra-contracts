use crate::contract::{query_total_bluna_issued, query_total_stluna_issued, slashing};
use crate::math::decimal_division;
use crate::state::{read_config, read_current_batch, read_parameters, read_state, store_state};
use cosmwasm_std::{
    to_binary, Api, Coin, CosmosMsg, Env, Extern, HandleResponse, Querier, QueryRequest,
    StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20HandleMsg;
use std::ops::AddAssign;
use validators_registry::common::calculate_delegations;
use validators_registry::msg::{HandleMsg as HandleMsgValidators, QueryMsg as QueryValidators};
use validators_registry::registry::Validator;

pub fn handle_bond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> Result<HandleResponse, StdError> {
    let params = read_parameters(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    // current batch requested fee is need for accurate exchange rate computation.
    let current_batch = read_current_batch(&deps.storage).load()?;
    let requested_with_fee = current_batch.requested_bluna_with_fee;

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if env.message.sent_funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    // coin must have be sent along with transaction and it should be in underlying coin denom
    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;

    // check slashing
    slashing(deps, env.clone())?;

    let state = read_state(&deps.storage).load()?;
    let sender = env.message.sender.clone();

    // get the total supply
    let mut total_supply = query_total_bluna_issued(&deps).unwrap_or_default();

    // peg recovery fee should be considered

    let mint_amount = decimal_division(payment.amount, state.bluna_exchange_rate);
    let mut mint_amount_with_fee = mint_amount;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = mint_amount * recovery_fee;
        let required_peg_fee =
            ((total_supply + mint_amount + current_batch.requested_bluna_with_fee)
                - (state.total_bond_bluna_amount + payment.amount))?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        mint_amount_with_fee = (mint_amount - peg_fee)?;
    }

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount_with_fee;

    // exchange rate should be updated for future
    store_state(&mut deps.storage).update(|mut prev_state| {
        prev_state.total_bond_bluna_amount += payment.amount;
        prev_state.update_bluna_exchange_rate(total_supply, requested_with_fee);
        Ok(prev_state)
    })?;

    let config = read_config(&deps.storage).load()?;
    let validators_registry_contract = if let Some(v) = config.validators_registry_contract {
        v
    } else {
        return Err(StdError::generic_err(
            "Validators registry contract address is empty",
        ));
    };
    let mut validators: Vec<Validator> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&validators_registry_contract)?,
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {})?,
        }))?;

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
        validators[i].total_delegated.add_assign(delegations[i]);
    }

    if !external_call_msgs.is_empty() {
        external_call_msgs.push(cosmwasm_std::CosmosMsg::Wasm(
            cosmwasm_std::WasmMsg::Execute {
                contract_addr: deps.api.human_address(&validators_registry_contract)?,
                msg: to_binary(&HandleMsgValidators::UpdateTotalDelegated {
                    updated_validators: validators,
                })?,
                send: vec![],
            },
        ));
    }

    let mint_msg = Cw20HandleMsg::Mint {
        recipient: sender,
        amount: mint_amount_with_fee,
    };

    let config = read_config(&deps.storage).load()?;
    let token_address = deps.api.human_address(
        &config
            .bluna_token_contract
            .expect("the token contract must have been registered"),
    )?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&mint_msg)?,
        send: vec![],
    }));

    let res = HandleResponse {
        messages: external_call_msgs,
        data: None,
        log: vec![],
    };
    Ok(res)
}

pub fn handle_bond_stluna<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let params = read_parameters(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if env.message.sent_funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;

    // check slashing
    slashing(deps, env.clone())?;

    let state = read_state(&deps.storage).load()?;
    let sender = env.message.sender.clone();

    // get the total supply
    let mut total_supply = query_total_stluna_issued(&deps).unwrap_or_default();

    let mint_amount = decimal_division(payment.amount, state.stluna_exchange_rate);

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount;

    // exchange rate should be updated for future
    store_state(&mut deps.storage).update(|mut prev_state| {
        prev_state.total_bond_stluna_amount += payment.amount;
        prev_state.update_stluna_exchange_rate(total_supply);
        Ok(prev_state)
    })?;

    let config = read_config(&deps.storage).load()?;
    let validators_registry_contract = if let Some(v) = config.validators_registry_contract {
        v
    } else {
        return Err(StdError::generic_err(
            "Validators registry contract address is empty",
        ));
    };
    let mut validators: Vec<Validator> =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: deps.api.human_address(&validators_registry_contract)?,
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {})?,
        }))?;

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
        validators[i].total_delegated.add_assign(delegations[i]);
    }

    if !external_call_msgs.is_empty() {
        external_call_msgs.push(cosmwasm_std::CosmosMsg::Wasm(
            cosmwasm_std::WasmMsg::Execute {
                contract_addr: deps.api.human_address(&validators_registry_contract)?,
                msg: to_binary(&HandleMsgValidators::UpdateTotalDelegated {
                    updated_validators: validators,
                })?,
                send: vec![],
            },
        ));
    }

    let mint_msg = Cw20HandleMsg::Mint {
        recipient: sender,
        amount: mint_amount,
    };

    let config = read_config(&deps.storage).load()?;
    let token_address = deps.api.human_address(
        &config
            .stluna_token_contract
            .expect("the token contract must have been registered"),
    )?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&mint_msg)?,
        send: vec![],
    }));

    let res = HandleResponse {
        messages: external_call_msgs,
        data: None,
        log: vec![],
    };
    Ok(res)
}
