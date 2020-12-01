use crate::init::RewardInitMsg;
use crate::msg::{HandleMsg, QueryMsg, TokenInfoResponse};
use crate::state::{
    config, config_read, index_read, index_store, params, params_read, pending_reward_read,
    pending_reward_store, prev_balance, prev_balance_read, read_holder_map, store_holder_map,
    Config, Index, Parameters,
};
use cosmwasm_std::{
    from_binary, log, to_binary, Api, BankMsg, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal,
    Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier, QueryRequest, StdError,
    StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};

use basset::deduct_tax;
use cosmwasm_storage::to_length_prefixed;
use gov_courier::PoolInfo;
use std::ops::Add;
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: RewardInitMsg,
) -> StdResult<InitResponse> {
    let conf = Config {
        owner: deps.api.canonical_address(&env.message.sender)?,
    };
    config(&mut deps.storage).save(&conf)?;

    let index = Index {
        global_index: Decimal::zero(),
    };
    index_store(&mut deps.storage).save(&index)?;

    prev_balance(&mut deps.storage).save(&Uint128::zero())?;

    store_holder_map(&mut deps.storage, env.message.sender, index.global_index)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Some(init_hook) = msg.init_hook {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: init_hook.contract_addr,
            msg: init_hook.msg,
            send: vec![],
        }));
    }

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    match msg {
        HandleMsg::ClaimRewards { recipient } => handle_claim_rewards(deps, env, recipient),
        HandleMsg::SwapToRewardDenom {} => handle_swap(deps, env),
        HandleMsg::UpdateGlobalIndex {} => handle_global_index(deps, env),
        HandleMsg::UpdateUserIndex { address, is_send } => {
            handle_update_index(deps, env, address, is_send)
        }
        HandleMsg::UpdateParams { swap_denom } => handle_update_params(deps, env, swap_denom),
    }
}

pub fn handle_claim_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: Option<HumanAddr>,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let receiver: HumanAddr;

    let config = config_read(&deps.storage).load()?;
    let owner_human = deps.api.human_address(&config.owner)?;

    match recipient.clone() {
        Some(value) => {
            receiver = value;
        }
        None => {
            receiver = env.message.sender;
        }
    }

    let is_exist = read_holder_map(&deps.storage, receiver.clone());
    if is_exist.is_err() {
        return Err(StdError::generic_err(
            "The sender has not requested any tokens",
        ));
    }

    let rcvr_raw = deps.api.canonical_address(&receiver)?;
    //calculate the reward
    let recv_index = read_holder_map(&deps.storage, receiver.clone())?;
    let global_index = index_read(&deps.storage).load()?.global_index;

    let mut msgs: Vec<CosmosMsg<TerraMsgWrapper>> = vec![];
    let mut reward = Uint128::zero();

    let pending_reward = pending_reward_read(&deps.storage)
        .load(rcvr_raw.as_slice())
        .unwrap_or_default();

    // if the recipient is none  which means the message is send by a user and
    // the receiver index has not been changed, it means there is not reward
    // this will help to manage transfers while the global index has not been updated
    if recipient.is_none() && global_index == recv_index && pending_reward.is_zero() {
        return Err(StdError::generic_err("There is no reward yet for the user"));
    }

    let contr_addr = env.contract.address;
    if global_index > recv_index || !pending_reward.is_zero() {
        let token_address = deps
            .api
            .human_address(&query_token_contract(&deps, owner_human)?)?;

        let balance = query_balance(&deps, &receiver, token_address)?;
        reward = calculate_reward(global_index, recv_index, balance)?;

        //set the pending reward to zero
        pending_reward_store(&mut deps.storage).save(rcvr_raw.as_slice(), &Uint128(0))?;

        //store the new index of holder map
        store_holder_map(&mut deps.storage, receiver.clone(), global_index)?;

        let final_reward = reward + pending_reward;

        prev_balance(&mut deps.storage).update(|prev_bal| prev_bal - final_reward)?;

        let swap_denom = params_read(&deps.storage).load()?.swap_denom;

        msgs.push(
            BankMsg::Send {
                from_address: contr_addr.clone(),
                to_address: receiver,
                amount: vec![deduct_tax(
                    &deps,
                    Coin {
                        denom: swap_denom,
                        amount: final_reward,
                    },
                )?],
            }
            .into(),
        );
    }

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "send_reward"),
            log("from", contr_addr),
            log("amount", reward),
        ],
        data: None,
    };

    Ok(res)
}

// calculate the reward based on the sender's index and the global index.
fn calculate_reward(
    general_index: Decimal,
    user_index: Decimal,
    user_balance: Uint128,
) -> StdResult<Uint128> {
    (general_index * user_balance) - (user_index * user_balance)
}

pub fn handle_swap<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let contr_addr = env.contract.address.clone();
    let balance = deps.querier.query_all_balances(env.contract.address)?;
    let mut msgs: Vec<CosmosMsg<TerraMsgWrapper>> = Vec::new();

    let swap_denom = params_read(&deps.storage).load()?.swap_denom;

    for coin in balance {
        if coin.denom == swap_denom {
            continue;
        }
        msgs.push(create_swap_msg(
            contr_addr.clone(),
            coin,
            swap_denom.to_string(),
        ));
    }

    let res = HandleResponse {
        messages: msgs,
        log: vec![log("action", "swap"), log("from", contr_addr)],
        data: None,
    };
    Ok(res)
}

pub fn handle_global_index<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    //check who sent this
    let config = config_read(&deps.storage).load()?;
    let owner_human = deps.api.human_address(&config.owner)?;
    let sender = env.message.sender;

    let token_address = deps
        .api
        .human_address(&query_token_contract(&deps, owner_human.clone())?)?;

    if sender != owner_human && sender != token_address {
        return Err(StdError::unauthorized());
    }

    let swap_denom = params_read(&deps.storage).load()?.swap_denom;

    //check the balance of the reward contract.
    let balance = deps
        .querier
        .query_balance(env.contract.address, &*swap_denom)
        .unwrap();

    let total_supply = {
        let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: token_address,
            key: Binary::from(to_length_prefixed(b"token_info")),
        }))?;
        let token_info: TokenInfoResponse = from_binary(&res)?;
        token_info.total_supply
    };

    let past_balance = prev_balance_read(&deps.storage).load()?;
    let claimed_reward = (balance.amount - past_balance)?;

    prev_balance(&mut deps.storage).save(&balance.amount)?;

    //error if there is no reward yet
    if claimed_reward.is_zero() {
        return Err(StdError::generic_err("There is no reward yet"));
    }

    //update the global index
    index_store(&mut deps.storage).update(|mut index| {
        index.global_index = index
            .global_index
            .add(Decimal::from_ratio(claimed_reward, total_supply.u128()));
        Ok(index)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_index")],
        data: None,
    };

    Ok(res)
}

pub fn handle_update_index<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    address: HumanAddr,
    is_send: Option<Uint128>,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = config_read(&deps.storage).load()?;
    let owner_human = deps.api.human_address(&config.owner)?;
    let address_raw = deps.api.canonical_address(&address)?;
    let sender = env.message.sender;

    let token_address = deps
        .api
        .human_address(&query_token_contract(&deps, owner_human.clone())?)?;

    if sender != owner_human && sender != token_address {
        return Err(StdError::unauthorized());
    }

    let global_index = index_read(&deps.storage).load()?.global_index;
    match is_send {
        Some(value) => {
            let prev_rcpt_balance = value;

            let rcpt_index = read_holder_map(&deps.storage, address.clone())?;
            let reward = calculate_reward(global_index, rcpt_index, prev_rcpt_balance)?;

            //store the reward
            pending_reward_store(&mut deps.storage)
                .update(address_raw.as_slice(), |balance: Option<Uint128>| {
                    Ok(balance.unwrap_or_default() + reward)
                })?;

            store_holder_map(&mut deps.storage, address, global_index)?;
        }
        None => {
            //save the holder map
            store_holder_map(&mut deps.storage, address, global_index)?;
        }
    }

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "register_holder")],
        data: None,
    };

    Ok(res)
}

pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    swap_denom: String,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = config_read(&deps.storage).load()?;
    let owner_human = deps.api.human_address(&config.owner)?;
    let sender = env.message.sender;

    if sender != owner_human {
        return Err(StdError::unauthorized());
    }

    let parameter = Parameters { swap_denom };

    params(&mut deps.storage).save(&parameter)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_params")],
        data: None,
    };
    Ok(res)
}

#[inline]
fn concat(namespace: &[u8], key: &[u8]) -> Vec<u8> {
    let mut k = namespace.to_vec();
    k.extend_from_slice(key);
    k
}

fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: &HumanAddr,
    contract_addr: HumanAddr,
) -> StdResult<Uint128> {
    let res: Binary = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr,
            key: Binary::from(concat(
                &to_length_prefixed(b"balance").to_vec(),
                (deps.api.canonical_address(&address)?).as_slice(),
            )),
        }))
        .unwrap_or_else(|_| to_binary(&Uint128::zero()).unwrap());

    let bal: Uint128 = from_binary(&res)?;
    Ok(bal)
}

fn query_token_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    contract_addr: HumanAddr,
) -> StdResult<CanonicalAddr> {
    let res: Binary = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr,
            key: Binary::from(to_length_prefixed(b"pool_info")),
        }))
        .unwrap();

    let pool_info: PoolInfo = from_binary(&res)?;
    Ok(pool_info.token_account)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::AccruedRewards { address } => to_binary(&query_accrued_rewards(&deps, address)?),
        QueryMsg::GlobalIndex {} => to_binary(&query_index(&deps)?),
        QueryMsg::UserIndex { address } => to_binary(&query_user_index(&deps, address)?),
        QueryMsg::PendingRewards { address } => to_binary(&query_user_pending(&deps, address)?),
    }
}

fn query_accrued_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {
    let global_index = index_read(&deps.storage).load()?.global_index;
    let owner_human = deps
        .api
        .human_address(&config_read(&deps.storage).load()?.owner)?;
    let token_address = deps
        .api
        .human_address(&query_token_contract(&deps, owner_human)?)?;
    let user_balance = query_balance(&deps, &address, token_address)?;
    let sender_reward_index = read_holder_map(&deps.storage, address.clone());
    if sender_reward_index.is_err() {
        return Err(StdError::generic_err("There is no user with this address"));
    }
    let reward = calculate_reward(global_index, sender_reward_index.unwrap(), user_balance)?;

    let address_raw = deps.api.canonical_address(&address).unwrap();
    let pending_reward = pending_reward_read(&deps.storage)
        .load(address_raw.as_slice())
        .unwrap_or_default();

    Ok(pending_reward + reward)
}

fn query_index<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Decimal> {
    let a = index_read(&deps.storage).load()?;
    Ok(a.global_index)
}

fn query_user_index<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Decimal> {
    let holder = read_holder_map(&deps.storage, address)?;
    Ok(holder)
}

fn query_user_pending<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {
    let address_raw = deps.api.canonical_address(&address).unwrap();
    let pending_reward = pending_reward_read(&deps.storage)
        .load(address_raw.as_slice())
        .unwrap_or_default();
    Ok(pending_reward)
}
