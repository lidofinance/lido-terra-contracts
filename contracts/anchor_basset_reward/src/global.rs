use crate::state::{read_config, read_state, store_state, Config, State};

use cosmwasm_std::{
    log, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, Querier, StdError, StdResult,
    Storage, Uint128,
};
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};

/// Swap all native tokens to reward_denom
/// Permissionless
pub fn handle_swap<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let contr_addr = env.contract.address;
    let balance = deps.querier.query_all_balances(contr_addr.clone())?;
    let mut msgs: Vec<CosmosMsg<TerraMsgWrapper>> = Vec::new();

    let reward_denom = read_config(&deps.storage)?.reward_denom;
    for coin in balance {
        if coin.denom == reward_denom {
            continue;
        }

        msgs.push(create_swap_msg(
            contr_addr.clone(),
            coin,
            reward_denom.to_string(),
        ));
    }

    let res = HandleResponse {
        messages: msgs,
        log: vec![log("action", "swap")],
        data: None,
    };
    Ok(res)
}

/// Increase global_index according to claimed rewards amount
/// Only hub_contract is allowed to execute
pub fn handle_update_global_index<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    prev_balance: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config: Config = read_config(&deps.storage)?;
    let mut state: State = read_state(&deps.storage)?;

    // Permission check
    if config.hub_contract != deps.api.canonical_address(&env.message.sender)? {
        return Err(StdError::unauthorized());
    }

    // Zero staking balance check
    if state.total_balance.is_zero() {
        return Err(StdError::generic_err("zero staking balance"));
    }

    let reward_denom = read_config(&deps.storage)?.reward_denom;

    // Load the reward contract balance
    let balance = deps
        .querier
        .query_balance(env.contract.address, reward_denom.as_str())
        .unwrap();

    // claimed_rewards = current_balance - prev_balance;
    let claimed_rewards = (balance.amount - prev_balance)?;

    // global_index += claimed_rewards / total_balance;
    state.global_index =
        state.global_index + Decimal::from_ratio(claimed_rewards, state.total_balance);
    store_state(&mut deps.storage, &state)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "update_global_index"),
            log("claimed_rewards", claimed_rewards),
        ],
        data: None,
    };

    Ok(res)
}
