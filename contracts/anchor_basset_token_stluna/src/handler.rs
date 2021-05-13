use cosmwasm_std::{
    Api, Binary, Env, Extern, HandleResponse, HandleResult, HumanAddr, Querier, Storage, Uint128,
};

use cw20_base::allowances::{
    handle_burn_from as cw20_burn_from, handle_send_from as cw20_send_from,
    handle_transfer_from as cw20_transfer_from,
};
use cw20_base::contract::{
    handle_burn as cw20_burn, handle_mint as cw20_mint, handle_send as cw20_send,
    handle_transfer as cw20_transfer,
};

pub fn handle_transfer<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_transfer(deps, env, recipient, amount)?;
    Ok(HandleResponse::default())
}

pub fn handle_burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
) -> HandleResult {
    cw20_burn(deps, env, amount)?;
    Ok(HandleResponse::default())
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_mint(deps, env, recipient, amount)?;
    Ok(HandleResponse::default())
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    cw20_send(deps, env, contract, amount, msg)?;
    Ok(HandleResponse::default())
}

pub fn handle_transfer_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_transfer_from(deps, env, owner, recipient, amount)?;
    Ok(HandleResponse::default())
}

pub fn handle_burn_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_burn_from(deps, env, owner, amount)?;
    Ok(HandleResponse::default())
}

pub fn handle_send_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    cw20_send_from(deps, env, owner, contract, amount, msg)?;
    Ok(HandleResponse::default())
}
