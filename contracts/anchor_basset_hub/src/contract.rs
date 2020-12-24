use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, Coin, CosmosMsg, Decimal, Env, Extern,
    HandleResponse, HumanAddr, InitResponse, Querier, QueryRequest, StakingMsg, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use crate::config::{
    handle_deactivate, handle_deregister_validator, handle_register_contracts,
    handle_register_validator, handle_update_config, handle_update_params,
};
use crate::msg::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, ExchangeRateResponse, InitMsg,
    QueryMsg, StateResponse, UnbondBatchesResponse, UnbondRequestsResponse,
    WhitelistedValidatorsResponse, WithdrawableUnbondedResponse,
};
use crate::state::{
    all_unbond_history, get_unbond_requests, get_unbond_requests_batches,
    query_get_finished_amount, read_config, read_current_batch, read_msg_status, read_parameters,
    read_state, read_valid_validators, read_validators, store_config, store_current_batch,
    store_msg_status, store_parameters, store_state, CurrentBatch, MsgStatus, Parameters,
};
use crate::unbond::{handle_unbond, handle_withdraw_unbonded};

use crate::bond::handle_bond;
use anchor_basset_reward::msg::HandleMsg::{SwapToRewardDenom, UpdateGlobalIndex};
use cosmwasm_storage::to_length_prefixed;
use cw20::Cw20ReceiveMsg;
use cw20_base::state::TokenInfo;
use hub_querier::{Config, State};
use hub_querier::{Cw20HookMsg, HandleMsg};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let sender = env.message.sender;
    let sndr_raw = deps.api.canonical_address(&sender)?;

    // store config
    let data = Config {
        creator: sndr_raw,
        reward_contract: None,
        token_contract: None,
    };
    store_config(&mut deps.storage).save(&data)?;

    // store state
    let state = State {
        exchange_rate: Decimal::one(),
        last_index_modification: env.block.time,
        last_unbonded_time: env.block.time,
        last_processed_batch: 0u64,
        ..Default::default()
    };

    store_state(&mut deps.storage).save(&state)?;

    // instantiate msg status
    let msg_state = MsgStatus {
        slashing: None,
        unbond: None,
    };
    store_msg_status(&mut deps.storage).save(&msg_state)?;

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
        id: 0,
        requested_with_fee: Default::default(),
    };
    store_current_batch(&mut deps.storage).save(&batch)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::Bond { validator } => handle_bond(deps, env, validator),
        HandleMsg::UpdateGlobalIndex {} => handle_update_global(deps, env),
        HandleMsg::WithdrawUnbonded {} => handle_withdraw_unbonded(deps, env),
        HandleMsg::RegisterSubcontracts {
            contract,
            contract_address,
        } => handle_register_contracts(deps, env, contract, contract_address),
        HandleMsg::RegisterValidator { validator } => {
            handle_register_validator(deps, env, validator)
        }
        HandleMsg::DeregisterValidator { validator } => {
            handle_deregister_validator(deps, env, validator)
        }
        HandleMsg::CheckSlashing {} => handle_slashing(deps, env),
        HandleMsg::UpdateParams {
            epoch_period,
            underlying_coin_denom: coin_denom,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
            reward_denom,
        } => handle_update_params(
            deps,
            env,
            epoch_period,
            coin_denom,
            unbonding_period,
            peg_recovery_fee,
            er_threshold,
            reward_denom,
        ),
        HandleMsg::DeactivateMsg { msg } => handle_deactivate(deps, env, msg),
        HandleMsg::UpdateConfig {
            owner,
            reward_contract,
            token_contract,
        } => handle_update_config(deps, env, owner, reward_contract, token_contract),
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
        Err(StdError::generic_err("Invalid request"))
    }
}

/// Update general parameters
/// Permissionless
pub fn handle_update_global<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let config = read_config(&deps.storage).load()?;
    let reward_addr = deps.api.human_address(
        &config
            .reward_contract
            .expect("the reward contract must have been registered"),
    )?;

    // Retrieve all validators
    let validators: Vec<HumanAddr> = read_validators(&deps.storage)?;

    // Send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(validators);
    messages.append(&mut withdraw_msgs);

    // Send Swap message to reward contract
    let swap_msg = SwapToRewardDenom {};
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.clone(),
        msg: to_binary(&swap_msg).unwrap(),
        send: vec![],
    }));

    // Send UpdateGlobalIndex message to reward contract
    // with prev_balance of reward denom
    let params: Parameters = read_parameters(&deps.storage).load()?;
    let prev_balance: Coin = deps
        .querier
        .query_balance(reward_addr.clone(), params.reward_denom.as_str())?;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr,
        msg: to_binary(&UpdateGlobalIndex {
            prev_balance: prev_balance.amount,
        })
        .unwrap(),
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
fn withdraw_all_rewards(validators: Vec<HumanAddr>) -> Vec<CosmosMsg> {
    let mut messages: Vec<CosmosMsg> = vec![];
    for val in validators {
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient: None,
        });
        messages.push(msg)
    }
    messages
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
    let mut actual_total_bonded = Uint128::zero();
    let delegations = deps.querier.query_all_delegations(env.contract.address)?;
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

/// Handler for tracking slashing
pub fn handle_slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    // Slashing status must be active to operate this message
    let msg_status = read_msg_status(&deps.storage).load()?;
    if msg_status.slashing.is_some() {
        return Err(StdError::generic_err(
            "this message is temporarily deactivated",
        ));
    }
    slashing(deps, env)?;
    Ok(HandleResponse::default())
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(&deps)?),
        QueryMsg::State {} => to_binary(&query_state(&deps)?),
        QueryMsg::ExchangeRate {} => to_binary(&query_exchange_rate(&deps)?),
        QueryMsg::CurrentBatch {} => to_binary(&query_current_batch(&deps)?),
        QueryMsg::WhitelistedValidators {} => to_binary(&query_white_validators(&deps)?),
        QueryMsg::WithdrawableUnbonded {
            address,
            block_time,
        } => to_binary(&query_withdrawable_unbonded(&deps, address, block_time)?),
        QueryMsg::Parameters {} => to_binary(&query_params(&deps)?),
        QueryMsg::UnbondRequests { address } => to_binary(&query_unbond_requests(&deps, address)?),
        QueryMsg::UnbondBatches { address } => to_binary(&query_user_batches(&deps, address)?),
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
    Ok(ConfigResponse {
        owner: deps.api.human_address(&config.creator)?,
        reward_contract: reward,
        token_contract: token,
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

fn query_exchange_rate<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ExchangeRateResponse> {
    let state = read_state(&deps.storage).load()?;
    let ex_rate = ExchangeRateResponse {
        rate: state.exchange_rate,
    };
    Ok(ex_rate)
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

fn query_user_batches<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<UnbondBatchesResponse> {
    let requests = get_unbond_requests_batches(&deps.storage, address)?;
    let res = UnbondBatchesResponse {
        unbond_batches: requests,
    };
    Ok(res)
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

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::HumanAddr;

    #[test]
    pub fn proper_withdraw_all() {
        let mut validators: Vec<HumanAddr> = vec![];
        for i in 0..10 {
            let address = format!("{}{}", "addr", i.to_string());
            validators.push(HumanAddr::from(address));
        }
        let res = withdraw_all_rewards(validators);
        assert_eq!(res.len(), 10);
        for i in 1..10 {
            match res.get(i).unwrap() {
                CosmosMsg::Staking(StakingMsg::Withdraw {
                    validator: val,
                    recipient: _,
                }) => {
                    let address = format!("{}{}", "addr", i.to_string());
                    assert_eq!(val, &HumanAddr::from(address));
                }
                _ => panic!("Unexpected message: {:?}", res.get(i).unwrap()),
            }
        }
    }
}
