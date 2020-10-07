use cosmwasm_std::{
    log, Api, Decimal, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg,
    StdError, StdResult, Storage, Uint128,
};

use crate::msg::{HandleMsg, InitMsg};
use crate::state::{
    balances, token_info, token_info_read, token_state, token_state_read, TokenInfo, pool_info, pool_info_read, PoolInfo
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

    let added_amount = payment.amount.add(amount);

    token.total_supply += amount;

    token_info(&mut deps.storage).save(&token)?;

    //update pool_info
    let mut pool = pool_info_read(&deps.storage).load()?;
    pool.total_bond_amount += amount;
    pool.total_issued += amount;

    let reward_index = pool.reward_index;

    pool_info(&mut deps.storage).save(&pool)?;

    let mut sub_env = env.clone();
    sub_env.message.sender = env.contract.address.clone();

    // Issue the bluna token for sender
    //TODO: Apply exchange rate before issuing bluna.
    let sender = sub_env.message.sender.clone();
    let rcpt_raw = deps.api.canonical_address(&sender)?;
    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

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
