#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, from_binary, to_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, DistributionMsg,
    Env, MessageInfo, QueryRequest, Response, StakingMsg, StdError, StdResult, Uint128, WasmMsg,
    WasmQuery,
};

use crate::config::{execute_update_config, execute_update_params};
use crate::state::{
    all_unbond_history, get_unbond_requests, migrate_unbond_history, migrate_unbond_wait_lists,
    query_get_finished_amount, read_validators, remove_whitelisted_validators_store, CONFIG,
    CURRENT_BATCH, OLD_CONFIG, OLD_CURRENT_BATCH, OLD_STATE, PARAMETERS, STATE,
};
use crate::unbond::{execute_unbond, execute_unbond_stluna, execute_withdraw_unbonded};

use crate::bond::execute_bond;
use crate::convert::{convert_bluna_stluna, convert_stluna_bluna};
use anchor_basset_rewards_dispatcher::msg::ExecuteMsg::{DispatchRewards, SwapToRewardDenom};
use anchor_basset_validators_registry::msg::ExecuteMsg::AddValidator;
use anchor_basset_validators_registry::registry::Validator;
use basset::hub::ExecuteMsg::SwapHook;
use basset::hub::{
    AllHistoryResponse, BondType, Config, ConfigResponse, CurrentBatch, CurrentBatchResponse,
    InstantiateMsg, MigrateMsg, Parameters, QueryMsg, State, StateResponse, UnbondRequestsResponse,
    WithdrawableUnbondedResponse,
};
use basset::hub::{Cw20HookMsg, ExecuteMsg};
use cosmwasm_storage::to_length_prefixed;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw20_base::state::TokenInfo;

#[cfg_attr(not(feature = "library"), entry_point)]
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
        last_index_modification: env.block.time.seconds(),
        last_unbonded_time: env.block.time.seconds(),
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
            redelegations,
        } => execute_redelegate_proxy(deps, env, info, src_validator, redelegations),
    }
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

    if sender_contract_addr != validators_registry_contract && sender_contract_addr != conf.creator
    {
        return Err(StdError::generic_err("unauthorized"));
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
    let contract_addr = deps.api.addr_canonicalize(&info.sender.as_str())?;

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
                execute_unbond(deps, env, info, cw20_msg.amount, cw20_msg.sender)
            } else if contract_addr == stluna_contract_addr {
                execute_unbond_stluna(deps, env, info, cw20_msg.amount, cw20_msg.sender)
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

    // Send Swap message to reward contract
    let swap_msg = SwapToRewardDenom {
        stluna_total_mint_amount: query_total_stluna_issued(deps.as_ref())?,
        bluna_total_mint_amount: query_total_bluna_issued(deps.as_ref())?,
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
    // Check the actual bonded amount
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
    if delegations.is_empty() {
        return Ok(());
    }

    //read params
    let params = PARAMETERS.load(deps.storage)?;
    let coin_denom = params.underlying_coin_denom;

    let mut actual_total_bonded = Uint128::zero();
    for delegation in &delegations {
        if delegation.amount.denom == coin_denom {
            actual_total_bonded += delegation.amount.amount;
        }
    }

    let state = STATE.load(deps.storage)?;
    // Check the amount that contract thinks is bonded
    let state_total_bonded = state.total_bond_bluna_amount + state.total_bond_stluna_amount;
    if state_total_bonded.is_zero() {
        return Ok(());
    }

    // Slashing happens if the expected amount is less than stored amount
    if state_total_bonded.u128() <= actual_total_bonded.u128() {
        return Ok(());
    }

    let bluna_bond_ratio = Decimal::from_ratio(state.total_bond_bluna_amount, state_total_bonded);

    // Need total issued for updating the exchange rate
    let bluna_total_issued = query_total_bluna_issued(deps.as_ref())?;
    let stluna_total_issued = query_total_stluna_issued(deps.as_ref())?;
    let current_batch = CURRENT_BATCH.load(deps.storage)?;
    let current_requested_bluna_with_fee = current_batch.requested_bluna_with_fee;
    let current_requested_stluna = current_batch.requested_stluna;

    STATE.update(deps.storage, |mut state| -> StdResult<_> {
        state.total_bond_bluna_amount = actual_total_bonded * bluna_bond_ratio;
        state.total_bond_stluna_amount =
            actual_total_bonded.checked_sub(state.total_bond_bluna_amount)?;

        state.update_bluna_exchange_rate(bluna_total_issued, current_requested_bluna_with_fee);
        state.update_stluna_exchange_rate(stluna_total_issued, current_requested_stluna);
        Ok(state)
    })?;

    Ok(())
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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::State {} => to_binary(&query_state(deps)?),
        QueryMsg::CurrentBatch {} => to_binary(&query_current_batch(deps)?),
        QueryMsg::WithdrawableUnbonded { address } => {
            to_binary(&query_withdrawable_unbonded(deps, address, env)?)
        }
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
    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address.to_string(),
        key: Binary::from(to_length_prefixed("token_info".as_bytes())),
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
    let token_info: TokenInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address.to_string(),
        key: Binary::from("token_info".as_bytes()),
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

#[cfg_attr(not(feature = "library"), entry_point)]
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
    let mut messages: Vec<CosmosMsg> = vec![];

    let add_validators_messsages: StdResult<Vec<CosmosMsg>> = whitelisted_validators
        .iter()
        .map(|validator_address| {
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: msg.validators_registry_contract.clone(),
                msg: if let Ok(m) = to_binary(&AddValidator {
                    validator: Validator {
                        total_delegated: Default::default(),
                        address: validator_address.clone(),
                    },
                }) {
                    m
                } else {
                    return Err(StdError::generic_err("failed to binary encode message"));
                },
                funds: vec![],
            }))
        })
        .collect();
    messages.extend_from_slice(&add_validators_messsages?);

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
