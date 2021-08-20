use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, DistributionMsg,
    Env, MessageInfo, QueryRequest, Response, StakingMsg, StdError, StdResult, Uint128, WasmMsg,
    WasmQuery,
};

use crate::config::{execute_update_config, execute_update_params};
use crate::math::decimal_division;
use crate::state::{
    all_unbond_history, get_unbond_requests, migrate_unbond_history, migrate_unbond_wait_lists,
    query_get_finished_amount, read_validators, remove_whitelisted_validators_store, CONFIG,
    CURRENT_BATCH, OLD_CONFIG, OLD_CURRENT_BATCH, OLD_STATE, PARAMETERS, STATE,
};
use crate::unbond::{execute_unbond, execute_unbond_stluna, execute_withdraw_unbonded};

use crate::bond::execute_bond_stluna;
use crate::bond::{execute_bond, execute_bond_rewards};
use anchor_basset_rewards_dispatcher::msg::ExecuteMsg::{DispatchRewards, SwapToRewardDenom};
use anchor_basset_validators_registry::msg::ExecuteMsg::AddValidator;
use anchor_basset_validators_registry::registry::Validator;
use basset::hub::ExecuteMsg::SwapHook;
use basset::hub::{
    AllHistoryResponse, Config, ConfigResponse, CurrentBatch, CurrentBatchResponse, InstantiateMsg,
    MigrateMsg, Parameters, QueryMsg, State, StateResponse, UnbondRequestsResponse,
    WithdrawableUnbondedResponse,
};
use basset::hub::{Cw20HookMsg, ExecuteMsg};
use cosmwasm_storage::to_length_prefixed;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw20_base::state::TokenInfo;
use std::ops::Mul;

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let sender = info.sender;
    let sndr_raw = deps.api.addr_canonicalize(&sender.as_str())?;

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
        last_index_modification: env.block.time.nanos(),
        last_unbonded_time: env.block.time.nanos(),
        last_processed_batch: 0u64,
        ..Default::default()
    };

    STATE.save(deps.storage, &state)?;

    // instantiate parameters
    let params = Parameters {
        epoch_period: msg.epoch_period,
        underlying_coin_denom: msg.underlying_coin_denom,
        unbonding_period: msg.unbonding_period,
        peg_recovery_fee: msg.peg_recovery_fee,
        er_threshold: msg.er_threshold,
        reward_denom: msg.reward_denom,
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

pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::Bond {} => execute_bond(deps, env, info),
        ExecuteMsg::BondForStLuna {} => execute_bond_stluna(deps, env, info),
        ExecuteMsg::BondRewards {} => execute_bond_rewards(deps, env, info),
        ExecuteMsg::UpdateGlobalIndex { airdrop_hooks } => {
            execute_update_global(deps, env, info, airdrop_hooks)
        }
        ExecuteMsg::WithdrawUnbonded {} => execute_withdraw_unbonded(deps, env, info),
        ExecuteMsg::CheckSlashing {} => execute_slashing(deps, env, info),
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
            dst_validator,
            amount,
        } => execute_redelegate_proxy(deps, env, info, src_validator, dst_validator, amount),
    }
}

pub fn execute_redelegate_proxy(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    src_validator: String,
    dst_validator: String,
    amount: Coin,
) -> StdResult<Response> {
    let sender_contract_addr = info.sender;
    let conf = CONFIG.load(deps.storage)?;
    let validators_registry_contract =
        deps.api
            .addr_humanize(&conf.validators_registry_contract.ok_or_else(|| {
                StdError::generic_err("the validator registry contract must have been registered")
            })?)?;
    if sender_contract_addr != validators_registry_contract {
        return Err(StdError::generic_err("unauthorized"));
    }
    let messages: Vec<CosmosMsg> = vec![cosmwasm_std::CosmosMsg::Staking(StakingMsg::Redelegate {
        src_validator,
        dst_validator,
        amount,
    })];

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
    let contract_addr = info.sender.clone();

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Unbond {} => {
            // only token contract can execute this message
            let conf = CONFIG.load(deps.storage)?;
            if deps.api.addr_canonicalize(&contract_addr.as_str())?
                == conf
                    .bluna_token_contract
                    .expect("the token contract must have been registered")
            {
                execute_unbond(deps, env, info, cw20_msg.amount, cw20_msg.sender)
            } else if deps.api.addr_canonicalize(&contract_addr.as_str())?
                == conf
                    .stluna_token_contract
                    .expect("the token contract must have been registered")
            {
                execute_unbond_stluna(deps, env, info, cw20_msg.amount, cw20_msg.sender)
            } else {
                Err(StdError::generic_err("unauthorized"))
            }
        }
        Cw20HookMsg::Convert {} => {
            let conf = CONFIG.load(deps.storage)?;
            if deps.api.addr_canonicalize(&contract_addr.as_str())?
                == conf
                    .bluna_token_contract
                    .expect("the token contract must have been registered")
            {
                convert_bluna_stluna(deps, env, cw20_msg.amount, cw20_msg.sender)
            } else if deps.api.addr_canonicalize(&contract_addr.as_str())?
                == conf
                    .stluna_token_contract
                    .expect("the token contract must have been registered")
            {
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
    let mut messages: Vec<CosmosMsg> = vec![];

    let config = CONFIG.load(deps.storage)?;
    let reward_addr = deps.api.addr_humanize(
        &config
            .reward_dispatcher_contract
            .expect("the reward contract must have been registered"),
    )?;

    if airdrop_hooks.is_some() {
        let registry_addr = deps
            .api
            .addr_humanize(&config.airdrop_registry_contract.unwrap())?;
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

    // Send Swap message to reward contract
    let swap_msg = SwapToRewardDenom {
        stluna_total_mint_amount: query_total_stluna_issued(deps.as_ref())?,
        bluna_total_mint_amount: query_total_bluna_issued(deps.as_ref())?,
    };

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.to_string(),
        msg: to_binary(&swap_msg).unwrap(),
        funds: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.to_string(),
        msg: to_binary(&DispatchRewards {}).unwrap(),
        funds: vec![],
    }));

    //update state last modified
    STATE.update(deps.storage, |mut last_state| -> StdResult<_> {
        last_state.last_index_modification = env.block.time.nanos();
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

    if delegations.is_empty() {
        Ok(messages)
    } else {
        for delegation in delegations {
            let msg: CosmosMsg =
                CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward {
                    validator: delegation.validator,
                });
            messages.push(msg);
        }
        Ok(messages)
    }
}

/// Check whether slashing has happened
/// This is used for checking slashing while bonding or unbonding
pub fn slashing(deps: &mut DepsMut, env: Env, _info: MessageInfo) -> StdResult<()> {
    //read params
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;

    let state = STATE.load(deps.storage)?;

    // Check the amount that contract thinks is bonded
    let state_total_bonded = state.total_bond_bluna_amount + state.total_bond_stluna_amount;
    if state_total_bonded.is_zero() {
        return Ok(());
    }

    let bluna_bond_ratio = Decimal::from_ratio(state.total_bond_bluna_amount, state_total_bonded);
    let stluna_bond_ratio = Decimal::from_ratio(state.total_bond_stluna_amount, state_total_bonded);

    // Check the actual bonded amount
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
    if delegations.is_empty() {
        Ok(())
    } else {
        let mut actual_total_bonded = Uint128::zero();
        for delegation in &delegations {
            if delegation.amount.denom == coin_denom {
                actual_total_bonded += delegation.amount.amount;
            }
        }

        // Need total issued for updating the exchange rate
        let bluna_total_issued = query_total_bluna_issued(deps.as_ref())?;
        let stluna_total_issued = query_total_stluna_issued(deps.as_ref())?;
        let current_batch = CURRENT_BATCH.load(deps.storage)?;
        let current_requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
        let current_requested_stluna = current_batch.requested_stluna;

        // Slashing happens if the expected amount is less than stored amount
        if state_total_bonded.u128() > actual_total_bonded.u128() {
            STATE.update(deps.storage, |mut state| -> StdResult<_> {
                state.total_bond_bluna_amount = actual_total_bonded * bluna_bond_ratio;
                state.total_bond_stluna_amount = actual_total_bonded * stluna_bond_ratio;

                state.update_bluna_exchange_rate(
                    bluna_total_issued,
                    current_requested_bluna_with_fee,
                );
                state.update_stluna_exchange_rate(stluna_total_issued, current_requested_stluna);
                Ok(state)
            })?;
        }

        Ok(())
    }
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
    let conf = CONFIG.load(deps.storage)?;

    let sender_raw = deps.api.addr_canonicalize(&info.sender.as_str())?;

    let airdrop_reg_raw = conf.airdrop_registry_contract.unwrap();
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
    if info.sender != env.contract.address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let airdrop_token_balance: Uint128 = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: airdrop_token_contract.to_string(),
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                (deps.api.addr_canonicalize(env.contract.address.as_str())?).as_slice(),
            )),
        }))
        .unwrap_or_else(|_| Uint128::zero());

    if airdrop_token_balance == Uint128::zero() {
        return Err(StdError::generic_err(format!(
            "There is no balance for {} in airdrop token contract {}",
            &env.contract.address, &airdrop_token_contract
        )));
    }
    let messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: airdrop_token_contract.clone(),
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: airdrop_swap_contract,
            amount: airdrop_token_balance,
            msg: swap_msg,
        })?,
        funds: vec![],
    })];

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap_airdrop_token"),
        attr("token_contract", airdrop_token_contract),
        attr("swap_amount", airdrop_token_balance),
    ]))
}

/// Handler for tracking slashing
pub fn execute_slashing(mut deps: DepsMut, env: Env, info: MessageInfo) -> StdResult<Response> {
    // call slashing
    slashing(&mut deps, env, info)?;
    // read state for log
    let state = STATE.load(deps.storage)?;

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

fn convert_stluna_bluna(
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
        bluna_mint_amount_with_fee = bluna_to_mint - peg_fee;
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

fn convert_bluna_stluna(
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

    let denom_equiv = state.bluna_exchange_rate.mul(bluna_amount);

    let stluna_to_mint = decimal_division(denom_equiv, state.stluna_exchange_rate);
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
    let requested_stluna_with_fee = current_batch.requested_stluna;

    let total_bluna_supply = query_total_bluna_issued(deps.as_ref())?;
    let total_stluna_supply = query_total_stluna_issued(deps.as_ref())?;
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

pub fn query(deps: Deps, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::CurrentBatch {} => to_binary(&query_current_batch(deps)?),
        QueryMsg::WithdrawableUnbonded {
            address,
            block_time,
        } => to_binary(&query_withdrawable_unbonded(deps, address, block_time)?),
        QueryMsg::Parameters {} => to_binary(&query_params(deps)?),
        QueryMsg::UnbondRequests { address } => to_binary(&query_unbond_requests(deps, address)?),
        QueryMsg::AllHistory { start_from, limit } => {
            to_binary(&query_unbond_requests_limitation(deps, start_from, limit)?)
        }
    }
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
                .addr_humanize(&config.reward_dispatcher_contract.unwrap())
                .unwrap()
                .to_string(),
        );
    }
    if config.bluna_token_contract.is_some() {
        bluna_token = Some(
            deps.api
                .addr_humanize(&config.bluna_token_contract.unwrap())
                .unwrap()
                .to_string(),
        );
    }
    if config.stluna_token_contract.is_some() {
        stluna_token = Some(
            deps.api
                .addr_humanize(&config.stluna_token_contract.unwrap())
                .unwrap()
                .to_string(),
        );
    }
    if config.validators_registry_contract.is_some() {
        validators_contract = Some(
            deps.api
                .addr_humanize(&config.validators_registry_contract.unwrap())
                .unwrap()
                .to_string(),
        );
    }
    if config.airdrop_registry_contract.is_some() {
        airdrop = Some(
            deps.api
                .addr_humanize(&config.airdrop_registry_contract.unwrap())
                .unwrap()
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

fn query_state(deps: Deps) -> StdResult<StateResponse> {
    let state = STATE.load(deps.storage)?;
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
    block_time: u64,
) -> StdResult<WithdrawableUnbondedResponse> {
    let params = PARAMETERS.load(deps.storage)?;
    let historical_time = block_time - params.unbonding_period;
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
            .expect("token contract must have been registered"),
    )?;
    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address.to_string(),
        key: Binary::from("\u{0}\ntoken_info".as_bytes()),
    }))?;
    Ok(token_info.total_supply)
}

pub(crate) fn query_total_stluna_issued(deps: Deps) -> StdResult<Uint128> {
    let token_address = deps.api.addr_humanize(
        &CONFIG
            .load(deps.storage)?
            .stluna_token_contract
            .expect("token contract must have been registered"),
    )?;
    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address.to_string(),
        key: Binary::from("\u{0}\ntoken_info".as_bytes()),
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

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}

pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> StdResult<Response> {
    // migrate state
    let old_state = OLD_STATE.load(deps.storage)?;
    let new_state = State {
        bluna_exchange_rate: old_state.exchange_rate,
        stluna_exchange_rate: Decimal::one(),
        total_bond_bluna_amount: old_state.total_bond_amount,
        total_bond_stluna_amount: Uint128::zero(),
        last_index_modification: old_state.last_index_modification,
        prev_hub_balance: old_state.prev_hub_balance,
        last_unbonded_time: old_state.last_unbonded_time,
        last_processed_batch: old_state.last_processed_batch,
    };
    STATE.save(deps.storage, &new_state)?;

    //migrate config
    let old_config = OLD_CONFIG.load(deps.storage)?;
    let new_config = Config {
        creator: old_config.creator,
        reward_dispatcher_contract: Some(
            deps.api
                .addr_canonicalize(&msg.reward_dispatcher_contract)?,
        ),
        validators_registry_contract: Some(
            deps.api
                .addr_canonicalize(&msg.validators_registry_contract)?,
        ),
        bluna_token_contract: old_config.token_contract,
        stluna_token_contract: Some(deps.api.addr_canonicalize(&msg.stluna_token_contract)?),
        airdrop_registry_contract: old_config.airdrop_registry_contract,
    };
    CONFIG.save(deps.storage, &new_config)?;

    //migrate CurrentBatch
    let old_current_batch = OLD_CURRENT_BATCH.load(deps.storage)?;
    let new_current_batch = CurrentBatch {
        id: old_current_batch.id,
        requested_bluna_with_fee: old_current_batch.requested_with_fee,
        requested_stluna: Uint128::zero(),
    };
    CURRENT_BATCH.save(deps.storage, &new_current_batch)?;

    //migrate whitelisted validators
    //we must add them to validators_registry_contract
    let whitelisted_validators = read_validators(deps.storage)?;
    let mut messages: Vec<CosmosMsg> = whitelisted_validators
        .iter()
        .map(|validator_address| {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: msg.validators_registry_contract.clone(),
                msg: to_binary(&AddValidator {
                    validator: Validator {
                        total_delegated: Default::default(),
                        address: validator_address.clone(),
                    },
                })
                .unwrap(),
                funds: vec![],
            })
        })
        .collect();
    remove_whitelisted_validators_store(deps.storage)?;

    let msg: CosmosMsg = CosmosMsg::Distribution(DistributionMsg::SetWithdrawAddress {
        address: msg.reward_dispatcher_contract,
    });
    messages.push(msg);

    // migrate unbond waitlist
    // update old values (Uint128) in PREFIX_WAIT_MAP storage to UnbondWaitEntity
    migrate_unbond_wait_lists(deps.storage)?;

    // migrate unbond history
    migrate_unbond_history(deps.storage)?;

    Ok(Response::new().add_messages(messages))
}
