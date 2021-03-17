use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, InitResponse, Querier, QueryRequest, StakingMsg, StdError, StdResult, Storage,
    Uint128, WasmMsg, WasmQuery,
};

use crate::config::{
    handle_deregister_validator, handle_register_validator, handle_update_config,
    handle_update_params,
};
use crate::msg::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, InitMsg, QueryMsg, StateResponse,
    UnbondRequestsResponse, WhitelistedValidatorsResponse, WithdrawableUnbondedResponse,
};
use crate::state::{
    all_unbond_history, get_unbond_requests, query_get_finished_amount, read_config,
    read_current_batch, read_parameters, read_state, read_valid_validators, store_config,
    store_current_batch, store_parameters, store_state, CurrentBatch, Parameters,
};
use crate::unbond::{handle_unbond, handle_withdraw_unbonded};

use crate::bond::handle_bond;
use cosmwasm_storage::to_length_prefixed;
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use cw20_base::state::TokenInfo;
use hub_querier::HandleMsg::SwapHook;
use hub_querier::{Config, State};
use hub_querier::{Cw20HookMsg, HandleMsg};
use reward_querier::HandleMsg::{SwapToRewardDenom, UpdateGlobalIndex};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let sender = env.message.sender;
    let sndr_raw = deps.api.canonical_address(&sender)?;

    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == msg.underlying_coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", "uluna"))
        })?;

    // store config
    let data = Config {
        creator: sndr_raw,
        reward_contract: None,
        token_contract: None,
        airdrop_registry_contract: None,
    };
    store_config(&mut deps.storage).save(&data)?;

    // store state
    let state = State {
        exchange_rate: Decimal::one(),
        last_index_modification: env.block.time,
        last_unbonded_time: env.block.time,
        last_processed_batch: 0u64,
        total_bond_amount: payment.amount,
        ..Default::default()
    };

    store_state(&mut deps.storage).save(&state)?;

    // instantiate parameters
    let params = Parameters {
        epoch_period: msg.epoch_period,
        underlying_coin_denom: msg.underlying_coin_denom,
        unbonding_period: msg.unbonding_period,
        peg_recovery_fee: msg.peg_recovery_fee,
        er_threshold: msg.er_threshold,
        reward_denom: msg.reward_denom,
    };

    store_parameters(&mut deps.storage).save(&params)?;

    let batch = CurrentBatch {
        id: 1,
        requested_with_fee: Default::default(),
    };
    store_current_batch(&mut deps.storage).save(&batch)?;

    let mut messages = vec![];

    // register the given validator
    let register_validator = HandleMsg::RegisterValidator {
        validator: msg.validator.clone(),
    };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&register_validator).unwrap(),
        send: vec![],
    }));

    // send the delegate message
    messages.push(CosmosMsg::Staking(StakingMsg::Delegate {
        validator: msg.validator.clone(),
        amount: payment.clone(),
    }));

    let res = InitResponse {
        messages,
        log: vec![
            log("register-validator", msg.validator),
            log("bond", payment.amount),
        ],
    };
    Ok(res)
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::Bond { validator } => handle_bond(deps, env, validator),
        HandleMsg::UpdateGlobalIndex { airdrop_hooks } => {
            handle_update_global(deps, env, airdrop_hooks)
        }
        HandleMsg::WithdrawUnbonded {} => handle_withdraw_unbonded(deps, env),
        HandleMsg::RegisterValidator { validator } => {
            handle_register_validator(deps, env, validator)
        }
        HandleMsg::DeregisterValidator { validator } => {
            handle_deregister_validator(deps, env, validator)
        }
        HandleMsg::CheckSlashing {} => handle_slashing(deps, env),
        HandleMsg::UpdateParams {
            epoch_period,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
        } => handle_update_params(
            deps,
            env,
            epoch_period,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
        ),
        HandleMsg::UpdateConfig {
            owner,
            reward_contract,
            token_contract,
            airdrop_registry_contract,
        } => handle_update_config(
            deps,
            env,
            owner,
            reward_contract,
            token_contract,
            airdrop_registry_contract,
        ),
        HandleMsg::SwapHook {
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        } => swap_hook(
            deps,
            env,
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        ),
        HandleMsg::ClaimAirdrop {
            airdrop_token_contract,
            airdrop_contract,
            airdrop_swap_contract,
            claim_msg,
            swap_msg,
        } => claim_airdrop(
            deps,
            env,
            airdrop_token_contract,
            airdrop_contract,
            airdrop_swap_contract,
            claim_msg,
            swap_msg,
        ),
    }
}

/// CW20 token receive handler.
pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<HandleResponse> {
    let contract_addr = env.message.sender.clone();

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::Unbond {} => {
                // only token contract can execute this message
                let conf = read_config(&deps.storage).load()?;
                if deps.api.canonical_address(&contract_addr)?
                    != conf
                        .token_contract
                        .expect("the token contract must have been registered")
                {
                    return Err(StdError::unauthorized());
                }
                handle_unbond(deps, env, cw20_msg.amount, cw20_msg.sender)
            }
        }
    } else {
        Err(StdError::generic_err(format!(
            "Invalid request: {message:?} message not included in request",
            message = "unbond"
        )))
    }
}

/// Update general parameters
/// Permissionless
pub fn handle_update_global<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_hooks: Option<Vec<Binary>>,
) -> StdResult<HandleResponse> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let config = read_config(&deps.storage).load()?;
    let reward_addr = deps.api.human_address(
        &config
            .reward_contract
            .expect("the reward contract must have been registered"),
    )?;

    if airdrop_hooks.is_some() {
        let registry_addr = deps
            .api
            .human_address(&config.airdrop_registry_contract.unwrap())?;
        for msg in airdrop_hooks.unwrap() {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: registry_addr.clone(),
                msg,
                send: vec![],
            }))
        }
    }

    // Send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(deps, env.contract.address.clone())?;
    messages.append(&mut withdraw_msgs);

    // Send Swap message to reward contract
    let swap_msg = SwapToRewardDenom {};
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.clone(),
        msg: to_binary(&swap_msg).unwrap(),
        send: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr,
        msg: to_binary(&UpdateGlobalIndex {}).unwrap(),
        send: vec![],
    }));

    //update state last modified
    store_state(&mut deps.storage).update(|mut last_state| {
        last_state.last_index_modification = env.block.time;
        Ok(last_state)
    })?;

    let res = HandleResponse {
        messages,
        log: vec![log("action", "update_global_index")],
        data: None,
    };
    Ok(res)
}

/// Create withdraw requests for all validators
fn withdraw_all_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    delegator: HumanAddr,
) -> StdResult<Vec<CosmosMsg>> {
    let mut messages: Vec<CosmosMsg> = vec![];
    let delegations = deps.querier.query_all_delegations(delegator);

    match delegations {
        Ok(delegations) => {
            if delegations.is_empty() {
                Ok(messages)
            } else {
                for delegation in delegations {
                    let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
                        validator: delegation.validator,
                        recipient: None,
                    });
                    messages.push(msg);
                }
                Ok(messages)
            }
        }
        Err(_) => Ok(messages),
    }
}

/// Check whether slashing has happened
/// This is used for checking slashing while bonding or unbonding
pub fn slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<()> {
    //read params
    let params = read_parameters(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;

    // Check the amount that contract thinks is bonded
    let state_total_bonded = read_state(&deps.storage).load()?.total_bond_amount;

    // Check the actual bonded amount
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
    if delegations.is_empty() {
        Ok(())
    } else {
        let mut actual_total_bonded = Uint128::zero();
        for delegation in delegations {
            if delegation.amount.denom == coin_denom {
                actual_total_bonded += delegation.amount.amount
            }
        }

        // Need total issued for updating the exchange rate
        let total_issued = query_total_issued(&deps)?;
        let current_requested_fee = read_current_batch(&deps.storage).load()?.requested_with_fee;

        // Slashing happens if the expected amount is less than stored amount
        if state_total_bonded.u128() > actual_total_bonded.u128() {
            store_state(&mut deps.storage).update(|mut state| {
                state.total_bond_amount = actual_total_bonded;
                state.update_exchange_rate(total_issued, current_requested_fee);
                Ok(state)
            })?;
        }

        Ok(())
    }
}

pub fn claim_airdrop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token_contract: HumanAddr,
    airdrop_contract: HumanAddr,
    airdrop_swap_contract: HumanAddr,
    claim_msg: Binary,
    swap_msg: Binary,
) -> StdResult<HandleResponse> {
    let conf = read_config(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let airdrop_reg_raw = conf.airdrop_registry_contract.unwrap();
    let airdrop_reg = deps.api.human_address(&airdrop_reg_raw)?;

    if airdrop_reg_raw != sender_raw {
        return Err(StdError::generic_err(format!(
            "Sender must be {}",
            airdrop_reg
        )));
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: airdrop_contract,
        msg: claim_msg,
        send: vec![],
    }));

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&SwapHook {
            airdrop_token_contract,
            airdrop_swap_contract,
            swap_msg,
        })?,
        send: vec![],
    }));

    Ok(HandleResponse {
        messages,
        log: vec![],
        data: None,
    })
}

pub fn swap_hook<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token_contract: HumanAddr,
    airdrop_swap_contract: HumanAddr,
    swap_msg: Binary,
) -> StdResult<HandleResponse> {
    if env.message.sender != env.contract.address {
        return Err(StdError::unauthorized());
    }

    let res: Binary = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: airdrop_token_contract.clone(),
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                (deps.api.canonical_address(&env.contract.address)?).as_slice(),
            )),
        }))
        .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());

    let airdrop_token_balance: Uint128 = from_binary(&res)?;

    if airdrop_token_balance == Uint128(0) {
        return Err(StdError::generic_err(format!(
            "There is no balance for {} in airdrop token contract {}",
            &env.contract.address, &airdrop_token_contract
        )));
    }
    let mut messages: Vec<CosmosMsg> = vec![];
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: airdrop_token_contract.clone(),
        msg: to_binary(&Cw20HandleMsg::Send {
            contract: airdrop_swap_contract,
            amount: airdrop_token_balance,
            msg: Some(swap_msg),
        })?,
        send: vec![],
    }));

    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "swap_airdrop_token"),
            log("token_contract", airdrop_token_contract),
            log("swap_amount", airdrop_token_balance),
        ],
        data: None,
    })
}

/// Handler for tracking slashing
pub fn handle_slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    // call slashing
    slashing(deps, env)?;
    // read state for log
    let state = read_state(&deps.storage).load()?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "check_slashing"),
            log("new_exchange_rate", state.exchange_rate),
        ],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(&deps)?),
        QueryMsg::State {} => to_binary(&query_state(&deps)?),
        QueryMsg::CurrentBatch {} => to_binary(&query_current_batch(&deps)?),
        QueryMsg::WhitelistedValidators {} => to_binary(&query_white_validators(&deps)?),
        QueryMsg::WithdrawableUnbonded {
            address,
            block_time,
        } => to_binary(&query_withdrawable_unbonded(&deps, address, block_time)?),
        QueryMsg::Parameters {} => to_binary(&query_params(&deps)?),
        QueryMsg::UnbondRequests { address } => to_binary(&query_unbond_requests(&deps, address)?),
        QueryMsg::AllHistory { start_from, limit } => {
            to_binary(&query_unbond_requests_limitation(&deps, start_from, limit)?)
        }
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage).load()?;
    let mut reward: Option<HumanAddr> = None;
    let mut token: Option<HumanAddr> = None;
    let mut airdrop: Option<HumanAddr> = None;
    if config.reward_contract.is_some() {
        reward = Some(
            deps.api
                .human_address(&config.reward_contract.unwrap())
                .unwrap(),
        );
    }
    if config.token_contract.is_some() {
        token = Some(
            deps.api
                .human_address(&config.token_contract.unwrap())
                .unwrap(),
        );
    }
    if config.airdrop_registry_contract.is_some() {
        airdrop = Some(
            deps.api
                .human_address(&config.airdrop_registry_contract.unwrap())
                .unwrap(),
        );
    }

    Ok(ConfigResponse {
        owner: deps.api.human_address(&config.creator)?,
        reward_contract: reward,
        token_contract: token,
        airdrop_registry_contract: airdrop,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateResponse> {
    let state = read_state(&deps.storage).load()?;
    let res = StateResponse {
        exchange_rate: state.exchange_rate,
        total_bond_amount: state.total_bond_amount,
        last_index_modification: state.last_index_modification,
        prev_hub_balance: state.prev_hub_balance,
        actual_unbonded_amount: state.actual_unbonded_amount,
        last_unbonded_time: state.last_unbonded_time,
        last_processed_batch: state.last_processed_batch,
    };
    Ok(res)
}

fn query_white_validators<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<WhitelistedValidatorsResponse> {
    let validators = read_valid_validators(&deps.storage)?;
    let response = WhitelistedValidatorsResponse { validators };
    Ok(response)
}

fn query_current_batch<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<CurrentBatchResponse> {
    let current_batch = read_current_batch(&deps.storage).load()?;
    Ok(CurrentBatchResponse {
        id: current_batch.id,
        requested_with_fee: current_batch.requested_with_fee,
    })
}

fn query_withdrawable_unbonded<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
    block_time: u64,
) -> StdResult<WithdrawableUnbondedResponse> {
    let params = read_parameters(&deps.storage).load()?;
    let historical_time = block_time - params.unbonding_period;
    let all_requests = query_get_finished_amount(&deps.storage, address, historical_time)?;

    let withdrawable = WithdrawableUnbondedResponse {
        withdrawable: all_requests,
    };
    Ok(withdrawable)
}

fn query_params<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Parameters> {
    read_parameters(&deps.storage).load()
}

pub(crate) fn query_total_issued<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Uint128> {
    let token_address = deps.api.human_address(
        &read_config(&deps.storage)
            .load()?
            .token_contract
            .expect("token contract must have been registered"),
    )?;
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address,
        key: Binary::from(to_length_prefixed(b"token_info")),
    }))?;
    let token_info: TokenInfo = from_binary(&res)?;
    Ok(token_info.total_supply)
}

fn query_unbond_requests<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<UnbondRequestsResponse> {
    let requests = get_unbond_requests(&deps.storage, address.clone())?;
    let res = UnbondRequestsResponse { address, requests };
    Ok(res)
}

fn query_unbond_requests_limitation<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<AllHistoryResponse> {
    let requests = all_unbond_history(&deps.storage, start, limit)?;
    let res = AllHistoryResponse { history: requests };
    Ok(res)
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}
