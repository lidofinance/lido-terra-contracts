use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

use cw20_legacy::allowances::{execute_decrease_allowance, execute_increase_allowance};
use cw20_legacy::contract::instantiate as cw20_init;
use cw20_legacy::contract::query as cw20_query;
use cw20_legacy::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};

use crate::handler::*;
use crate::msg::TokenInitMsg;
use crate::state::store_hub_contract;
use cw20::MinterResponse;
use cw20_legacy::ContractError;

pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: TokenInitMsg,
) -> StdResult<Response> {
    store_hub_contract(
        deps.storage,
        &deps.api.addr_canonicalize(&msg.hub_contract)?,
    )?;

    cw20_init(
        deps,
        env,
        info,
        InstantiateMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: msg.decimals,
            initial_balances: msg.initial_balances,
            mint: Some(MinterResponse {
                minter: msg.hub_contract,
                cap: None,
            }),
        },
    )?;

    Ok(Response::default())
}

pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
    }
}

pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    cw20_query(deps, _env, msg)
}
