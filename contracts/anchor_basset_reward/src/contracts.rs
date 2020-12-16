use crate::global::{handle_swap, handle_update_global_index};
use crate::msg::{ConfigResponse, HandleMsg, InitMsg, QueryMsg, StateResponse};
use crate::state::{read_config, read_state, store_config, store_state, Config, State};
use crate::user::{
    handle_claim_rewards, handle_decrease_balance, handle_increase_balance, query_accrued_rewards,
    query_holder, query_holders,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, Decimal, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, Uint128,
};

use terra_cosmwasm::TerraMsgWrapper;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let conf = Config {
        hub_contract: deps.api.canonical_address(&msg.hub_contract)?,
        swap_denom: msg.swap_denom,
    };

    store_config(&mut deps.storage, &conf)?;
    store_state(
        &mut deps.storage,
        &State {
            global_index: Decimal::zero(),
            total_balance: Uint128::zero(),
        },
    )?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    match msg {
        HandleMsg::UpdateConfig {
            hub_contract,
            swap_denom,
        } => handle_update_config(deps, env, hub_contract, swap_denom),
        HandleMsg::ClaimRewards { recipient } => handle_claim_rewards(deps, env, recipient),
        HandleMsg::SwapToRewardDenom {} => handle_swap(deps, env),
        HandleMsg::UpdateGlobalIndex { prev_balance } => {
            handle_update_global_index(deps, env, prev_balance)
        }
        HandleMsg::IncreaseBalance { address, amount } => {
            handle_increase_balance(deps, env, address, amount)
        }
        HandleMsg::DecreaseBalance { address, amount } => {
            handle_decrease_balance(deps, env, address, amount)
        }
    }
}

pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    hub_contract: Option<HumanAddr>,
    swap_denom: Option<String>,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let mut config = read_config(&deps.storage)?;
    if config.hub_contract != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    if let Some(hub_contract) = hub_contract {
        config.hub_contract = deps.api.canonical_address(&hub_contract)?;
    }

    if let Some(swap_denom) = swap_denom {
        config.swap_denom = swap_denom;
    }

    store_config(&mut deps.storage, &config)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    };
    Ok(res)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(&deps)?),
        QueryMsg::State {} => to_binary(&query_state(&deps)?),
        QueryMsg::AccruedRewards { address } => to_binary(&query_accrued_rewards(&deps, address)?),
        QueryMsg::Holder { address } => to_binary(&query_holder(&deps, address)?),
        QueryMsg::Holders { start_after, limit } => {
            to_binary(&query_holders(&deps, start_after, limit)?)
        }
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config: Config = read_config(&deps.storage)?;
    Ok(ConfigResponse {
        hub_contract: deps.api.human_address(&config.hub_contract)?,
        swap_denom: config.swap_denom,
    })
}

fn query_state<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<StateResponse> {
    let state: State = read_state(&deps.storage)?;
    Ok(StateResponse {
        global_index: state.global_index,
        total_balance: state.total_balance,
    })
}
