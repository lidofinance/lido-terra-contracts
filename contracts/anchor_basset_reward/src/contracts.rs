use crate::init::RewardInitMsg;
use crate::msg::{HandleMsg, QueryMsg};
use crate::state::{config, config_read, index_store, Config, Index};
use cosmwasm_std::{
    coins, log, Api, BankMsg, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use std::ops::Add;
use terra_cosmwasm::{create_swap_msg, TerraMsgWrapper};

const SWAP_DENOM: &str = "uusd";

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: RewardInitMsg,
) -> StdResult<InitResponse> {
    let conf = Config { owner: msg.owner };
    config(&mut deps.storage).save(&conf)?;

    let index = Index {
        global_index: Decimal::zero(),
    };
    index_store(&mut deps.storage).save(&index)?;

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
        HandleMsg::SendReward { receiver, amount } => handle_send(deps, env, receiver, amount),
        HandleMsg::Swap {} => handle_swap(deps, env),
        HandleMsg::UpdateGlobalIndex { total_supply } => {
            handle_global_index(deps, env, total_supply)
        }
    }
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    receiver: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    //check whether the gov contract has sent this
    let conf = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if conf.owner != sender_raw {
        return Err(StdError::generic_err("unauthorized"));
    }

    let contr_addr = env.contract.address;
    let msgs = vec![BankMsg::Send {
        from_address: contr_addr.clone(),
        to_address: receiver,
        amount: coins(Uint128::u128(&amount), "uusd"),
    }
    .into()];

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "send_reward"),
            log("from", contr_addr),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_swap<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let contr_addr = env.contract.address.clone();
    let balance = deps.querier.query_all_balances(env.contract.address)?;
    let mut msgs: Vec<CosmosMsg<TerraMsgWrapper>> = Vec::new();

    for coin in balance {
        msgs.push(create_swap_msg(
            contr_addr.clone(),
            coin,
            SWAP_DENOM.to_string(),
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
    total_supply: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    //TODO: Do we need to consider tax here?
    //check who sent this
    let config = config_read(&deps.storage).load()?;
    let owner_raw = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_raw {
        return Err(StdError::generic_err("Unauthorized"));
    }

    //check the balance of the reward contract.
    let balance = deps
        .querier
        .query_balance(env.contract.address, SWAP_DENOM)
        .unwrap();

    //update the global index
    index_store(&mut deps.storage).update(|mut index| {
        index.global_index = index.global_index.add(Decimal::from_ratio(
            balance.amount.u128(),
            total_supply.u128(),
        ));
        Ok(index)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_index")],
        data: None,
    };

    Ok(res)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>,
    _msg: QueryMsg,
) -> StdResult<Binary> {
    unimplemented!()
}
