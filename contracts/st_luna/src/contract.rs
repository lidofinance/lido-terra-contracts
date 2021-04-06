use cosmwasm_std::{
    Api, Binary, Env, Extern, HandleResponse, InitResponse, Querier, StdResult, Storage,
};

use cw20_base::allowances::{handle_decrease_allowance, handle_increase_allowance};
use cw20_base::contract::init as cw20_init;
use cw20_base::contract::query as cw20_query;
use cw20_base::msg::{HandleMsg, InitMsg, QueryMsg};

use crate::handler::*;
use crate::msg::TokenInitMsg;
use crate::state::store_hub_contract;
use cw20::MinterResponse;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: TokenInitMsg,
) -> StdResult<InitResponse> {
    cw20_init(
        deps,
        env,
        InitMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: msg.decimals,
            initial_balances: msg.initial_balances,
            mint: Some(MinterResponse {
                minter: msg.hub_contract.clone(),
                cap: None,
            }),
        },
    )?;

    store_hub_contract(
        &mut deps.storage,
        &deps.api.canonical_address(&msg.hub_contract)?,
    )?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Transfer { recipient, amount } => handle_transfer(deps, env, recipient, amount),
        HandleMsg::Burn { amount } => handle_burn(deps, env, amount),
        HandleMsg::Send {
            contract,
            amount,
            msg,
        } => handle_send(deps, env, contract, amount, msg),
        HandleMsg::Mint { recipient, amount } => handle_mint(deps, env, recipient, amount),
        HandleMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => handle_increase_allowance(deps, env, spender, amount, expires),
        HandleMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => handle_decrease_allowance(deps, env, spender, amount, expires),
        HandleMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => handle_transfer_from(deps, env, owner, recipient, amount),
        HandleMsg::BurnFrom { owner, amount } => handle_burn_from(deps, env, owner, amount),
        HandleMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => handle_send_from(deps, env, owner, contract, amount, msg),
    }
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    cw20_query(deps, msg)
}
