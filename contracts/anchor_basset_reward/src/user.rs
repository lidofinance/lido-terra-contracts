use crate::msg::{AccruedRewardsResponse, HolderResponse, HoldersResponse};
use crate::querier::query_token_contract;
use crate::state::{
    read_config, read_holder, read_holders, read_state, store_holder, store_state, Config, Holder,
    State,
};

use cosmwasm_std::{
    log, Api, BankMsg, Coin, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier, StdError,
    StdResult, Storage, Uint128,
};

use basset::deduct_tax;
use terra_cosmwasm::TerraMsgWrapper;

pub fn handle_claim_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: Option<HumanAddr>,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let contract_addr = env.contract.address;
    let holder_addr = env.message.sender.clone();
    let holder_addr_raw = deps.api.canonical_address(&holder_addr)?;
    let recipient = match recipient {
        Some(value) => value,
        None => env.message.sender,
    };

    let mut holder: Holder = read_holder(&deps.storage, &holder_addr_raw)?;
    let state: State = read_state(&deps.storage)?;
    let config: Config = read_config(&deps.storage)?;

    let rewards = calculate_rewards(state.global_index, holder.index, holder.balance)?
        + holder.pending_rewards;
    if rewards.is_zero() {
        return Err(StdError::generic_err("There is no reward yet for the user"));
    }

    holder.pending_rewards = Uint128::zero();
    holder.index = state.global_index;
    store_holder(&mut deps.storage, &holder_addr_raw, &holder)?;

    Ok(HandleResponse {
        messages: vec![BankMsg::Send {
            from_address: contract_addr,
            to_address: recipient,
            amount: vec![deduct_tax(
                &deps,
                Coin {
                    denom: config.reward_denom,
                    amount: rewards,
                },
            )?],
        }
        .into()],
        log: vec![
            log("action", "claim_reward"),
            log("holder_address", holder_addr),
            log("rewards", rewards),
        ],
        data: None,
    })
}

pub fn handle_increase_balance<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = read_config(&deps.storage)?;
    let owner_human = deps.api.human_address(&config.hub_contract)?;
    let address_raw = deps.api.canonical_address(&address)?;
    let sender = env.message.sender;

    let token_address = deps
        .api
        .human_address(&query_token_contract(&deps, owner_human)?)?;

    // Check sender is token contract
    if sender != token_address {
        return Err(StdError::unauthorized());
    }

    let mut state: State = read_state(&deps.storage)?;
    let mut holder: Holder = read_holder(&deps.storage, &address_raw)?;

    let rewards = calculate_rewards(state.global_index, holder.index, holder.balance)?;
    holder.index = state.global_index;
    holder.pending_rewards += rewards;
    holder.balance += amount;
    state.total_balance += amount;

    store_holder(&mut deps.storage, &address_raw, &holder)?;
    store_state(&mut deps.storage, &state)?;
    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "increase_balance"),
            log("holder_address", address),
            log("amount", amount),
        ],
        data: None,
    };

    Ok(res)
}

pub fn handle_decrease_balance<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = read_config(&deps.storage)?;
    let hub_contract = deps.api.human_address(&config.hub_contract)?;
    let address_raw = deps.api.canonical_address(&address)?;

    // Check sender is token contract
    if query_token_contract(&deps, hub_contract)?
        != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    let mut state: State = read_state(&deps.storage)?;
    let mut holder: Holder = read_holder(&deps.storage, &address_raw)?;
    if holder.balance < amount {
        return Err(StdError::generic_err(
            "cannot derease more than the user balance",
        ));
    }

    let rewards = calculate_rewards(state.global_index, holder.index, holder.balance)?;
    holder.index = state.global_index;
    holder.pending_rewards += rewards;
    holder.balance = (holder.balance - amount).unwrap();
    state.total_balance = (state.total_balance - amount).unwrap();

    store_holder(&mut deps.storage, &address_raw, &holder)?;
    store_state(&mut deps.storage, &state)?;
    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "decrease_balance"),
            log("holder_address", address),
            log("amount", amount),
        ],
        data: None,
    };

    Ok(res)
}

pub fn query_accrued_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<AccruedRewardsResponse> {
    let global_index = read_state(&deps.storage)?.global_index;
    let holder: Holder = read_holder(&deps.storage, &deps.api.canonical_address(&address)?)?;
    let rewards =
        calculate_rewards(global_index, holder.index, holder.balance)? + holder.pending_rewards;

    Ok(AccruedRewardsResponse { rewards })
}

pub fn query_holder<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<HolderResponse> {
    let holder: Holder = read_holder(&deps.storage, &deps.api.canonical_address(&address)?)?;
    Ok(HolderResponse {
        address,
        balance: holder.balance,
        index: holder.index,
        pending_rewards: holder.pending_rewards,
    })
}

pub fn query_holders<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<HumanAddr>,
    limit: Option<u32>,
) -> StdResult<HoldersResponse> {
    let start_after = if let Some(start_after) = start_after {
        Some(deps.api.canonical_address(&start_after)?)
    } else {
        None
    };

    let holders: Vec<HolderResponse> = read_holders(&deps, start_after, limit)?;

    Ok(HoldersResponse { holders })
}

// calculate the reward based on the sender's index and the global index.
fn calculate_rewards(
    general_index: Decimal,
    user_index: Decimal,
    user_balance: Uint128,
) -> StdResult<Uint128> {
    (general_index * user_balance) - (user_index * user_balance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn proper_calculate_rewards() {
        let global_index = Decimal::from_ratio(Uint128(9), Uint128(100));
        let user_index = Decimal::zero();
        let user_balance = Uint128(1000);
        let reward = calculate_rewards(global_index, user_index, user_balance).unwrap();
        assert_eq!(reward, Uint128(90));
    }
}
