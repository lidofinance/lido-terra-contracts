use crate::contract::{query_total_bluna_issued, query_total_stluna_issued, slashing};
use crate::math::decimal_division;
use crate::state::{CONFIG, CURRENT_BATCH, PARAMETERS, STATE};
use anchor_basset_validators_registry::common::calculate_delegations;
use anchor_basset_validators_registry::msg::QueryMsg as QueryValidators;
use anchor_basset_validators_registry::registry::Validator;
use cosmwasm_std::{
    to_binary, Coin, CosmosMsg, DepsMut, Env, MessageInfo, QueryRequest, Response, StakingMsg,
    StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};
use cw20::Cw20ExecuteMsg;

pub fn execute_bond(mut deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, StdError> {
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    // current batch requested fee is need for accurate exchange rate computation.
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_with_fee = current_batch.requested_bluna_with_fee;

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
    slashing(&mut deps, env.clone(), info.clone())?;

    let state = STATE.load(deps.storage)?;
    let sender = info.sender.clone();

    // get the total supply
    let mut total_supply = query_total_bluna_issued(deps.as_ref()).unwrap_or_default();

    // peg recovery fee should be considered

    let mint_amount = decimal_division(payment.amount, state.bluna_exchange_rate);
    let mut mint_amount_with_fee = mint_amount;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = mint_amount * recovery_fee;
        let required_peg_fee =
            (total_supply + mint_amount + current_batch.requested_bluna_with_fee)
                - (state.total_bond_bluna_amount + payment.amount);
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        mint_amount_with_fee = mint_amount - peg_fee;
    }

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount_with_fee;

    // exchange rate should be updated for future
    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        prev_state.total_bond_bluna_amount += payment.amount;
        prev_state.update_bluna_exchange_rate(total_supply, requested_with_fee);
        Ok(prev_state)
    })?;

    let config = CONFIG.load(deps.storage)?;
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

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender.to_string(),
        amount: mint_amount_with_fee,
    };

    let config = CONFIG.load(deps.storage)?;
    let token_address = deps.api.addr_humanize(
        &config
            .bluna_token_contract
            .expect("the token contract must have been registered"),
    )?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address.to_string(),
        msg: to_binary(&mint_msg)?,
        funds: vec![],
    }));

    let res = Response::new().add_messages(external_call_msgs);
    Ok(res)
}

pub fn execute_bond_stluna(mut deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if info.funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested = current_batch.requested_stluna;

    let payment = info
        .funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;

    // check slashing
    slashing(&mut deps, env.clone(), info.clone())?;

    let state = STATE.load(deps.storage)?;
    let sender = info.sender.clone();

    // get the total supply
    let mut total_supply = query_total_stluna_issued(deps.as_ref()).unwrap_or_default();

    let mint_amount = decimal_division(payment.amount, state.stluna_exchange_rate);

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount;

    // exchange rate should be updated for future
    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        prev_state.total_bond_stluna_amount += payment.amount;
        prev_state.update_stluna_exchange_rate(total_supply, requested);
        Ok(prev_state)
    })?;

    let config = CONFIG.load(deps.storage)?;
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

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender.to_string(),
        amount: mint_amount,
    };

    let config = CONFIG.load(deps.storage)?;
    let token_address = deps.api.addr_humanize(
        &config
            .stluna_token_contract
            .expect("the token contract must have been registered"),
    )?;

    external_call_msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address.to_string(),
        msg: to_binary(&mint_msg)?,
        funds: vec![],
    }));

    let res = Response::new().add_messages(external_call_msgs);
    Ok(res)
}

pub fn execute_bond_rewards(mut deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let reward_dispatcher_addr = deps.api.addr_humanize(
        &config
            .reward_dispatcher_contract
            .expect("the reward dispatcher contract must have been registered"),
    )?;
    if info.sender != reward_dispatcher_addr {
        return Err(StdError::generic_err("unauthorized"));
    }

    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if info.funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested = current_batch.requested_stluna;

    let payment = info
        .funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;

    // check slashing
    slashing(&mut deps, env.clone(), info.clone())?;

    let total_supply = query_total_stluna_issued(deps.as_ref())?;

    STATE.update(deps.storage, |mut prev_state| -> StdResult<_> {
        prev_state.total_bond_stluna_amount += payment.amount;
        prev_state.update_stluna_exchange_rate(total_supply, requested);
        Ok(prev_state)
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

    let res = Response::new().add_messages(external_call_msgs);
    Ok(res)
}
