use cosmwasm_std::{
    log, Api, Decimal, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg,
    StdError, StdResult, Storage, Uint128,
};

use crate::msg::{HandleMsg, InitMsg};
use crate::state::{
    balances, balances_read, pool_info, pool_info_read, token_info, token_info_read, token_state,
    token_state_read, PoolInfo, TokenInfo,
};
use std::ops::Add;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    // validate token info
    msg.validate()?;

    // store token info
    let initial_total_supply = Uint128::zero();
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: initial_total_supply,
    };
    token_info(&mut deps.storage).save(&data)?;

    let pool = PoolInfo::default();
    pool_info(&mut deps.storage).save(&pool)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Mint { validator, amount } => handle_mint(deps, env, validator, amount),
        HandleMsg::ClaimRewards {} => handle_reward(deps, env),
        HandleMsg::Send { recipient, amount } => handle_send(deps, env, recipient, amount),
        _ => Ok(HandleResponse::default()),
    }
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    //TODO: Check whether the account has this amount of Luna.

    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let mut token = token_info_read(&deps.storage).load()?;

    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == token.name)
        .ok_or_else(|| StdError::generic_err(format!("No {} tokens sent", &token.name)))?;

    //update pool_info
    let mut pool = pool_info_read(&deps.storage).load()?;
    pool.total_bond_amount += amount;
    pool.total_issued += amount;

    let reward_index = pool.reward_index;

    let amount_with_exchange_rate =
        if pool.total_bond_amount.is_zero() || pool.total_issued.is_zero() {
            amount
        } else {
            pool.update_exchange_rate();
            let exchange_rate = pool.exchange_rate;
            exchange_rate * amount
        };

    pool_info(&mut deps.storage).save(&pool)?;

    let mut sub_env = env.clone();
    sub_env.message.sender = env.contract.address.clone();

    // Issue the bluna token for sender
    let sender = sub_env.message.sender.clone();
    let rcpt_raw = deps.api.canonical_address(&sender)?;
    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount_with_exchange_rate)
    })?;

    let added_amount = payment.amount.add(amount);

    token.total_supply += amount_with_exchange_rate;

    token_info(&mut deps.storage).save(&token)?;

    let mut token_status = token_state_read(&deps.storage).load()?;

    token_status
        .delegation_map
        .insert(validator.clone(), amount);

    token_status.holder_map.insert(sender.clone(), reward_index);

    token_state(&mut deps.storage).save(&token_status)?;

    // bond them to the validator
    let res = HandleResponse {
        messages: vec![StakingMsg::Delegate {
            validator,
            amount: payment.clone(),
        }
        .into()],
        log: vec![
            log("action", "mint"),
            log("from", env.message.sender),
            log("bonded", payment.amount),
            log("minted", added_amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_reward<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let sender = env.message.sender.clone();
    let rcpt_raw = deps.api.canonical_address(&sender)?;

    let token = token_state_read(&deps.storage).load()?;
    if token.holder_map.get(&sender).is_none() {
        return Err(StdError::generic_err(
            "The sender has not requested any tokens",
        ));
    }
    let sender_reward_index = token
        .holder_map
        .get(&sender)
        .expect("The existence of the sender has been checked");

    let pool = pool_info_read(&deps.storage).load()?;
    let general_index = pool.reward_index;

    let user_balance = balances_read(&deps.storage).load(rcpt_raw.as_slice())?;

    let reward = calculate_reward(general_index, sender_reward_index, user_balance).unwrap();

    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + reward)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "claim_reward"),
            log("to", sender),
            log("amount", reward),
        ],
        data: None,
    };
    Ok(res)
}

pub fn calculate_reward(
    general_index: Decimal,
    user_index: &Decimal,
    user_balance: Uint128,
) -> StdResult<Uint128> {
    general_index * user_balance - *user_index * user_balance
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let mut accounts = balances(&mut deps.storage);
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    accounts.update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "send"),
            log("from", deps.api.human_address(&sender_raw)?),
            log("to", recipient),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}
