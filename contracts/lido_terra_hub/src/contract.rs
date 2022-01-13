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

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use std::string::FromUtf8Error;

use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, DistributionMsg,
    Env, MessageInfo, Order, QueryRequest, Response, StakingMsg, StdError, StdResult, Uint128,
    WasmMsg, WasmQuery,
};

use crate::config::{execute_update_config, execute_update_params};
use crate::state::{
    all_unbond_history, get_unbond_requests, query_get_finished_amount, CONFIG, CURRENT_BATCH,
    GUARDIANS, PARAMETERS, STATE,
};
use crate::unbond::{execute_unbond, execute_unbond_stluna, execute_withdraw_unbonded};

use crate::bond::execute_bond;
use crate::convert::{convert_bluna_stluna, convert_stluna_bluna};
use basset::hub::ExecuteMsg::SwapHook;
use basset::hub::{
    AllHistoryResponse, BondType, Config, ConfigResponse, CurrentBatch, CurrentBatchResponse,
    InstantiateMsg, MigrateMsg, Parameters, QueryMsg, State, StateResponse, UnbondRequestsResponse,
    WithdrawableUnbondedResponse,
};
use basset::hub::{Cw20HookMsg, ExecuteMsg};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};
use lido_terra_rewards_dispatcher::msg::ExecuteMsg::{DispatchRewards, SwapToRewardDenom};
use lido_terra_validators_registry::msg::QueryMsg as QueryValidators;
use lido_terra_validators_registry::registry::ValidatorResponse;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let sender = info.sender;
    let sndr_raw = deps.api.addr_canonicalize(sender.as_str())?;

    // store config
    let data = Config {
        creator: sndr_raw,
        reward_dispatcher_contract: None,
        validators_registry_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        stluna_token_contract: None,
    };
    CONFIG.save(deps.storage, &data)?;

    // store state
    let state = State {
        bluna_exchange_rate: Decimal::one(),
        stluna_exchange_rate: Decimal::one(),
        last_index_modification: env.block.time.seconds(),
        last_unbonded_time: env.block.time.seconds(),
        last_processed_batch: 0u64,
        ..Default::default()
    };

    STATE.save(deps.storage, &state)?;

    if msg.peg_recovery_fee.gt(&Decimal::one()) {
        return Err(StdError::generic_err(
            "peg_recovery_fee can not be greater than 1",
        ));
    }

    // instantiate parameters
    let params = Parameters {
        epoch_period: msg.epoch_period,
        underlying_coin_denom: msg.underlying_coin_denom,
        unbonding_period: msg.unbonding_period,
        peg_recovery_fee: msg.peg_recovery_fee,
        er_threshold: msg.er_threshold.min(Decimal::one()),
        reward_denom: msg.reward_denom,
        paused: Some(false),
    };

    PARAMETERS.save(deps.storage, &params)?;

    let batch = CurrentBatch {
        id: 1,
        requested_bluna_with_fee: Default::default(),
        requested_stluna: Default::default(),
    };
    CURRENT_BATCH.save(deps.storage, &batch)?;

    let res = Response::new();
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Bond {} => execute_bond(deps, env, info, BondType::BLuna),
        ExecuteMsg::BondForStLuna {} => execute_bond(deps, env, info, BondType::StLuna),
        ExecuteMsg::BondRewards {} => execute_bond(deps, env, info, BondType::BondRewards),
        ExecuteMsg::UpdateGlobalIndex { airdrop_hooks } => {
            execute_update_global(deps, env, info, airdrop_hooks)
        }
        ExecuteMsg::WithdrawUnbonded {} => execute_withdraw_unbonded(deps, env, info),
        ExecuteMsg::CheckSlashing {} => execute_slashing(deps, env),
        ExecuteMsg::UpdateParams {
            epoch_period,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
        } => execute_update_params(
            deps,
            env,
            info,
            epoch_period,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
        ),
        ExecuteMsg::UpdateConfig {
            owner,
            rewards_dispatcher_contract,
            bluna_token_contract,
            airdrop_registry_contract,
            validators_registry_contract,
            stluna_token_contract,
        } => execute_update_config(
            deps,
            env,
            info,
            owner,
            rewards_dispatcher_contract,
            bluna_token_contract,
            stluna_token_contract,
            airdrop_registry_contract,
            validators_registry_contract,
        ),
        ExecuteMsg::SwapHook {
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        } => swap_hook(
            deps,
            env,
            info,
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        ),
        ExecuteMsg::ClaimAirdrop {
            airdrop_token_contract,
            airdrop_contract,
            airdrop_swap_contract,
            claim_msg,
            swap_msg,
        } => claim_airdrop(
            deps,
            env,
            info,
            airdrop_token_contract,
            airdrop_contract,
            airdrop_swap_contract,
            claim_msg,
            swap_msg,
        ),
        ExecuteMsg::RedelegateProxy {
            src_validator,
            redelegations,
        } => execute_redelegate_proxy(deps, env, info, src_validator, redelegations),
        ExecuteMsg::PauseContracts {} => execute_pause_contracts(deps, env, info),
        ExecuteMsg::UnpauseContracts {} => execute_unpause_contracts(deps, env, info),
        ExecuteMsg::AddGuardians { addresses } => execute_add_guardians(deps, env, info, addresses),
        ExecuteMsg::RemoveGuardians { addresses } => {
            execute_remove_guardians(deps, env, info, addresses)
        }
    }
}

pub fn execute_add_guardians(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    guardians: Vec<String>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner = deps.api.addr_humanize(&config.creator)?;

    if info.sender != owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    for guardian in &guardians {
        GUARDIANS.save(deps.storage, guardian.clone(), &true)?;
    }

    Ok(Response::new()
        .add_attributes(vec![attr("action", "add_guardians")])
        .add_attributes(guardians.iter().map(|g| attr("value", g))))
}

pub fn execute_remove_guardians(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    guardians: Vec<String>,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner = deps.api.addr_humanize(&config.creator)?;

    if info.sender != owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    for guardian in &guardians {
        GUARDIANS.remove(deps.storage, guardian.clone());
    }

    Ok(Response::new()
        .add_attributes(vec![attr("action", "remove_guardians")])
        .add_attributes(guardians.iter().map(|g| attr("value", g))))
}

pub fn execute_pause_contracts(deps: DepsMut, _env: Env, info: MessageInfo) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner = deps.api.addr_humanize(&config.creator)?;

    if !(info.sender == owner || GUARDIANS.has(deps.storage, info.sender.to_string())) {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut params: Parameters = PARAMETERS.load(deps.storage)?;
    params.paused = Some(true);

    PARAMETERS.save(deps.storage, &params)?;

    let res = Response::new().add_attributes(vec![attr("action", "pause_contracts")]);
    Ok(res)
}

pub fn execute_unpause_contracts(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner = deps.api.addr_humanize(&config.creator)?;

    if info.sender != owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut params: Parameters = PARAMETERS.load(deps.storage)?;
    params.paused = Some(false);

    PARAMETERS.save(deps.storage, &params)?;

    let res = Response::new().add_attributes(vec![attr("action", "unpause_contracts")]);
    Ok(res)
}

pub fn execute_redelegate_proxy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    src_validator: String,
    redelegations: Vec<(String, Coin)>,
) -> StdResult<Response> {
    let sender_contract_addr = deps.api.addr_canonicalize(info.sender.as_str())?;
    let conf = CONFIG.load(deps.storage)?;
    let validators_registry_contract = conf.validators_registry_contract.ok_or_else(|| {
        StdError::generic_err("the validator registry contract must have been registered")
    })?;

    if !(sender_contract_addr == validators_registry_contract
        || sender_contract_addr == conf.creator)
    {
        return Err(StdError::generic_err("unauthorized"));
    }

    // If the message is not sent by the validators registry contract itself, check that
    // the destination validators are in the registry.
    if sender_contract_addr != validators_registry_contract {
        let validators: Vec<ValidatorResponse> =
            deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: deps
                    .api
                    .addr_humanize(&validators_registry_contract)?
                    .to_string(),
                msg: to_binary(&QueryValidators::GetValidatorsForDelegation {})?,
            }))?;

        let validators: Vec<String> = validators.into_iter().map(|x| x.address.clone()).collect();
        for (dst_validator_addr, _) in redelegations.clone() {
            if !validators.contains(&dst_validator_addr) {
                return Err(StdError::generic_err(format!(
                    "Redelegation validator {} is not in the registry",
                    dst_validator_addr
                )));
            }
        }
    }

    let messages: Vec<CosmosMsg> = redelegations
        .into_iter()
        .map(|(dst_validator, amount)| {
            cosmwasm_std::CosmosMsg::Staking(StakingMsg::Redelegate {
                src_validator: src_validator.clone(),
                dst_validator,
                amount,
            })
        })
        .collect();

    let res = Response::new().add_messages(messages);

    Ok(res)
}

/// CW20 token receive handler.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<Response> {
    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused.unwrap_or(false) {
        return Err(StdError::generic_err("the contract is temporarily paused"));
    }

    let contract_addr = deps.api.addr_canonicalize(info.sender.as_str())?;

    // only token contract can execute this message
    let conf = CONFIG.load(deps.storage)?;

    let bluna_contract_addr = if let Some(b) = conf.bluna_token_contract {
        b
    } else {
        return Err(StdError::generic_err(
            "the bLuna token contract must have been registered",
        ));
    };

    let stluna_contract_addr = if let Some(st) = conf.stluna_token_contract {
        st
    } else {
        return Err(StdError::generic_err(
            "the stLuna token contract must have been registered",
        ));
    };

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Unbond {} => {
            if contract_addr == bluna_contract_addr {
                execute_unbond(deps, env, cw20_msg.amount, cw20_msg.sender)
            } else if contract_addr == stluna_contract_addr {
                execute_unbond_stluna(deps, env, cw20_msg.amount, cw20_msg.sender)
            } else {
                Err(StdError::generic_err("unauthorized"))
            }
        }
        Cw20HookMsg::Convert {} => {
            if contract_addr == bluna_contract_addr {
                convert_bluna_stluna(deps, env, cw20_msg.amount, cw20_msg.sender)
            } else if contract_addr == stluna_contract_addr {
                convert_stluna_bluna(deps, env, cw20_msg.amount, cw20_msg.sender)
            } else {
                Err(StdError::generic_err("unauthorized"))
            }
        }
    }
}

/// Update general parameters
/// Permissionless
pub fn execute_update_global(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    airdrop_hooks: Option<Vec<Binary>>,
) -> StdResult<Response> {
    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused.unwrap_or(false) {
        return Err(StdError::generic_err("the contract is temporarily paused"));
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    let config = CONFIG.load(deps.storage)?;
    let reward_addr =
        deps.api
            .addr_humanize(&config.reward_dispatcher_contract.ok_or_else(|| {
                StdError::generic_err("the reward contract must have been registered")
            })?)?;

    if airdrop_hooks.is_some() {
        let registry_addr =
            deps.api
                .addr_humanize(&config.airdrop_registry_contract.ok_or_else(|| {
                    StdError::generic_err("the airdrop registry contract must have been registered")
                })?)?;
        for msg in airdrop_hooks.unwrap() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: registry_addr.to_string(),
                msg,
                funds: vec![],
            }))
        }
    }

    // Send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(&deps, env.contract.address.to_string())?;
    messages.append(&mut withdraw_msgs);

    let state = STATE.load(deps.storage)?;

    // Send Swap message to reward contract
    let swap_msg = SwapToRewardDenom {
        stluna_total_bonded: state.total_bond_stluna_amount,
        bluna_total_bonded: state.total_bond_bluna_amount,
    };

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.to_string(),
        msg: to_binary(&swap_msg)?,
        funds: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.to_string(),
        msg: to_binary(&DispatchRewards {})?,
        funds: vec![],
    }));

    //update state last modified
    STATE.update(deps.storage, |mut last_state| -> StdResult<_> {
        last_state.last_index_modification = env.block.time.seconds();
        Ok(last_state)
    })?;

    let res = Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "update_global_index")]);
    Ok(res)
}

/// Create withdraw requests for all validators
fn withdraw_all_rewards(deps: &DepsMut, delegator: String) -> StdResult<Vec<CosmosMsg>> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let delegations = deps.querier.query_all_delegations(delegator)?;

    if !delegations.is_empty() {
        for delegation in delegations {
            let msg: CosmosMsg =
                CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward {
                    validator: delegation.validator,
                });
            messages.push(msg);
        }
    }

    Ok(messages)
}

fn query_actual_state(deps: Deps, env: Env) -> StdResult<State> {
    let mut state = STATE.load(deps.storage)?;
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
    if delegations.is_empty() {
        return Ok(state);
    }

    //read params
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;

    // Check the actual bonded amount
    let mut actual_total_bonded = Uint128::zero();
    for delegation in &delegations {
        if delegation.amount.denom == coin_denom {
            actual_total_bonded += delegation.amount.amount;
        }
    }

    // Check the amount that contract thinks is bonded
    let state_total_bonded = state.total_bond_bluna_amount + state.total_bond_stluna_amount;
    if state_total_bonded.is_zero() {
        return Ok(state);
    }

    // Need total issued for updating the exchange rate
    let bluna_total_issued = query_total_bluna_issued(deps)?;
    let stluna_total_issued = query_total_stluna_issued(deps)?;
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let current_requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
    let current_requested_stluna = current_batch.requested_stluna;

    if state_total_bonded.u128() > actual_total_bonded.u128() {
        let bluna_bond_ratio =
            Decimal::from_ratio(state.total_bond_bluna_amount, state_total_bonded);
        state.total_bond_bluna_amount = actual_total_bonded * bluna_bond_ratio;
        state.total_bond_stluna_amount =
            actual_total_bonded.checked_sub(state.total_bond_bluna_amount)?;
    }
    state.update_bluna_exchange_rate(bluna_total_issued, current_requested_bluna_with_fee);
    state.update_stluna_exchange_rate(stluna_total_issued, current_requested_stluna);
    Ok(state)
}

/// Check whether slashing has happened
/// This is used for checking slashing while bonding or unbonding
pub fn slashing(deps: &mut DepsMut, env: Env) -> StdResult<State> {
    let state = query_actual_state(deps.as_ref(), env)?;

    STATE.save(deps.storage, &state)?;

    Ok(state)
}

#[allow(clippy::too_many_arguments)]
pub fn claim_airdrop(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    airdrop_token_contract: String,
    airdrop_contract: String,
    airdrop_swap_contract: String,
    claim_msg: Binary,
    swap_msg: Binary,
) -> StdResult<Response> {
    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused.unwrap_or(false) {
        return Err(StdError::generic_err("the contract is temporarily paused"));
    }

    let conf = CONFIG.load(deps.storage)?;

    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;

    let airdrop_reg_raw = if let Some(airdrop) = conf.airdrop_registry_contract {
        airdrop
    } else {
        return Err(StdError::generic_err("airdrop contract must be registered"));
    };

    let airdrop_reg = deps.api.addr_humanize(&airdrop_reg_raw)?;

    if airdrop_reg_raw != sender_raw {
        return Err(StdError::generic_err(format!(
            "Sender must be {}",
            airdrop_reg
        )));
    }

    let mut messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: airdrop_contract,
        msg: claim_msg,
        funds: vec![],
    })];

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        msg: to_binary(&SwapHook {
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages))
}

pub fn swap_hook(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    airdrop_token_contract: String,
    airdrop_swap_contract: String,
    swap_msg: Binary,
) -> StdResult<Response> {
    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused.unwrap_or(false) {
        return Err(StdError::generic_err("the contract is temporarily paused"));
    }

    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let airdrop_token_balance: BalanceResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: airdrop_token_contract.to_string(),
            msg: to_binary(&Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            })?,
        }))?;

    if airdrop_token_balance.balance == Uint128::zero() {
        return Err(StdError::generic_err(format!(
            "There is no balance for {} in airdrop token contract {}",
            &env.contract.address, &airdrop_token_contract
        )));
    }
    let messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: airdrop_token_contract.clone(),
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: airdrop_swap_contract,
            amount: airdrop_token_balance.balance,
            msg: swap_msg,
        })?,
        funds: vec![],
    })];

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap_airdrop_token"),
        attr("token_contract", airdrop_token_contract),
        attr("swap_amount", airdrop_token_balance.balance),
    ]))
}

/// Handler for tracking slashing
pub fn execute_slashing(mut deps: DepsMut, env: Env) -> StdResult<Response> {
    let params: Parameters = PARAMETERS.load(deps.storage)?;
    if params.paused.unwrap_or(false) {
        return Err(StdError::generic_err("the contract is temporarily paused"));
    }

    // call slashing and
    let state = slashing(&mut deps, env)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "check_slashing"),
        attr(
            "new_bluna_exchange_rate",
            state.bluna_exchange_rate.to_string(),
        ),
        attr(
            "new_stluna_exchange_rate",
            state.stluna_exchange_rate.to_string(),
        ),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps, env)?),
        QueryMsg::CurrentBatch {} => to_binary(&query_current_batch(deps)?),
        QueryMsg::WithdrawableUnbonded { address } => {
            to_binary(&query_withdrawable_unbonded(deps, address, env)?)
        }
        QueryMsg::Parameters {} => to_binary(&query_params(deps)?),
        QueryMsg::UnbondRequests { address } => to_binary(&query_unbond_requests(deps, address)?),
        QueryMsg::AllHistory { start_from, limit } => {
            to_binary(&query_unbond_requests_limitation(deps, start_from, limit)?)
        }
        QueryMsg::Guardians => to_binary(&query_guardians(deps)?),
    }
}

fn query_guardians(deps: Deps) -> StdResult<Vec<String>> {
    let guardians = GUARDIANS.keys(deps.storage, None, None, Order::Ascending);
    let guardians_decoded: Result<Vec<String>, FromUtf8Error> =
        guardians.map(String::from_utf8).collect();
    Ok(guardians_decoded?)
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let mut reward: Option<String> = None;
    let mut validators_contract: Option<String> = None;
    let mut bluna_token: Option<String> = None;
    let mut stluna_token: Option<String> = None;
    let mut airdrop: Option<String> = None;
    if config.reward_dispatcher_contract.is_some() {
        reward = Some(
            deps.api
                .addr_humanize(&config.reward_dispatcher_contract.unwrap())?
                .to_string(),
        );
    }
    if config.bluna_token_contract.is_some() {
        bluna_token = Some(
            deps.api
                .addr_humanize(&config.bluna_token_contract.unwrap())?
                .to_string(),
        );
    }
    if config.stluna_token_contract.is_some() {
        stluna_token = Some(
            deps.api
                .addr_humanize(&config.stluna_token_contract.unwrap())?
                .to_string(),
        );
    }
    if config.validators_registry_contract.is_some() {
        validators_contract = Some(
            deps.api
                .addr_humanize(&config.validators_registry_contract.unwrap())?
                .to_string(),
        );
    }
    if config.airdrop_registry_contract.is_some() {
        airdrop = Some(
            deps.api
                .addr_humanize(&config.airdrop_registry_contract.unwrap())?
                .to_string(),
        );
    }

    Ok(ConfigResponse {
        owner: deps.api.addr_humanize(&config.creator)?.to_string(),
        reward_dispatcher_contract: reward,
        validators_registry_contract: validators_contract,
        bluna_token_contract: bluna_token,
        airdrop_registry_contract: airdrop,
        stluna_token_contract: stluna_token,
    })
}

fn query_state(deps: Deps, env: Env) -> StdResult<StateResponse> {
    let state = query_actual_state(deps, env)?;
    let res = StateResponse {
        bluna_exchange_rate: state.bluna_exchange_rate,
        stluna_exchange_rate: state.stluna_exchange_rate,
        total_bond_bluna_amount: state.total_bond_bluna_amount,
        total_bond_stluna_amount: state.total_bond_stluna_amount,
        last_index_modification: state.last_index_modification,
        prev_hub_balance: state.prev_hub_balance,
        last_unbonded_time: state.last_unbonded_time,
        last_processed_batch: state.last_processed_batch,
    };
    Ok(res)
}

fn query_current_batch(deps: Deps) -> StdResult<CurrentBatchResponse> {
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    Ok(CurrentBatchResponse {
        id: current_batch.id,
        requested_bluna_with_fee: current_batch.requested_bluna_with_fee,
        requested_stluna: current_batch.requested_stluna,
    })
}

fn query_withdrawable_unbonded(
    deps: Deps,
    address: String,
    env: Env,
) -> StdResult<WithdrawableUnbondedResponse> {
    let params = PARAMETERS.load(deps.storage)?;
    let historical_time = env.block.time.seconds() - params.unbonding_period;
    let all_requests = query_get_finished_amount(deps.storage, address, historical_time)?;

    let withdrawable = WithdrawableUnbondedResponse {
        withdrawable: all_requests,
    };
    Ok(withdrawable)
}

fn query_params(deps: Deps) -> StdResult<Parameters> {
    PARAMETERS.load(deps.storage)
}

pub(crate) fn query_total_bluna_issued(deps: Deps) -> StdResult<Uint128> {
    let token_address = deps.api.addr_humanize(
        &CONFIG
            .load(deps.storage)?
            .bluna_token_contract
            .ok_or_else(|| StdError::generic_err("token contract must have been registered"))?,
    )?;
    let token_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token_address.to_string(),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;
    Ok(token_info.total_supply)
}

pub(crate) fn query_total_stluna_issued(deps: Deps) -> StdResult<Uint128> {
    let token_address = deps.api.addr_humanize(
        &CONFIG
            .load(deps.storage)?
            .stluna_token_contract
            .ok_or_else(|| StdError::generic_err("token contract must have been registered"))?,
    )?;
    let token_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: token_address.to_string(),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;
    Ok(token_info.total_supply)
}

fn query_unbond_requests(deps: Deps, address: String) -> StdResult<UnbondRequestsResponse> {
    let requests = get_unbond_requests(deps.storage, address.clone())?;
    let res = UnbondRequestsResponse { address, requests };
    Ok(res)
}

fn query_unbond_requests_limitation(
    deps: Deps,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<AllHistoryResponse> {
    let requests = all_unbond_history(deps.storage, start, limit)?;
    let res = AllHistoryResponse { history: requests };
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::new())
}
