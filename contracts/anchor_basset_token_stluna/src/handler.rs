use anchor_basset_token::msg::Cw20HookMsg;
use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, Querier, QueryRequest, StdError, StdResult, Storage, Uint128, WasmMsg,
    WasmQuery,
};

use cosmwasm_storage::to_length_prefixed;
use cw20_base::allowances::{
    handle_burn_from as cw20_burn_from, handle_send_from as cw20_send_from,
    handle_transfer_from as cw20_transfer_from,
};
use cw20_base::contract::{
    handle_burn as cw20_burn, handle_mint as cw20_mint, handle_send as cw20_send,
    handle_transfer as cw20_transfer,
};
use cw20_base::state::token_info_read;
use std::ops::Mul;

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};

use crate::state::read_hub_contract;

use hub_querier::{Config as HubConfig};
use anchor_basset_token::querier::query_exchange_rates;

pub fn handle_transfer<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_transfer(deps, env, recipient, amount)
}

pub fn handle_burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
) -> HandleResult {
    cw20_burn(deps, env, amount)
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_mint(deps, env, recipient, amount)
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    cw20_send(deps, env, contract, amount, msg)
}

pub fn handle_transfer_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_transfer_from(deps, env, owner, recipient, amount)
}

pub fn handle_burn_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    cw20_burn_from(deps, env, owner, amount)
}

pub fn handle_send_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    cw20_send_from(deps, env, owner, contract, amount, msg)
}

/// CW20 token receive handler.
pub fn handle_receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    let contract_addr = env.message.sender.clone();

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::Convert {} => {
                let hub_address = deps.api.human_address(&read_hub_contract(&deps.storage)?)?;
                let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
                    contract_addr: hub_address,
                    key: Binary::from(to_length_prefixed(b"config")),
                }))?;
                let conf: HubConfig = from_binary(&res)?;
                let bluna_token_address =
                    deps.api.human_address(&conf.bluna_token_contract.ok_or(
                        StdError::generic_err("the bluna token contract must have been registered"),
                    )?)?;
                // only token contract can execute this message
                if contract_addr != bluna_token_address {
                    Err(StdError::unauthorized())
                } else {
                    convert_bluna(
                        deps,
                        env,
                        cw20_msg.amount,
                        cw20_msg.sender,
                        bluna_token_address,
                    )
                }
            }
        }
    } else {
        Err(StdError::generic_err(format!(
            "Invalid request: {message:?} message not included in request",
            message = "convert"
        )))
    }
}

fn convert_bluna<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
    bluna_token_address: HumanAddr,
) -> HandleResult {
    let (bluna_exchange_rate, stluna_exchange_rate) = query_exchange_rates(deps)?;

    let bluna_equivalent = bluna_exchange_rate.mul(amount);
    let stluna_equivalent = stluna_exchange_rate.mul(amount);

    let value_ratio = Decimal::from_ratio(stluna_equivalent, bluna_equivalent);
    let stluna_to_mint = value_ratio.mul(amount);

    let mut messages: Vec<CosmosMsg> = vec![];
    let burn_msg = Cw20HandleMsg::Burn { amount };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: bluna_token_address,
        msg: to_binary(&burn_msg)?,
        send: vec![],
    }));

    let mint_res = handle_mint(
        deps,
        get_minter_env(deps, env)?,
        sender.clone(),
        stluna_to_mint,
    )?;

    let res = HandleResponse {
        messages: vec![messages, mint_res.messages].concat(),
        log: vec![
            log("action", "convert_bluna"),
            log("from", sender),
            log("bluna_exchange_rate", bluna_exchange_rate),
            log("stluna_exchange_rate", stluna_exchange_rate),
            log("bluna_amount", amount),
            log("stluna_amount", stluna_to_mint),
        ],
        data: None,
    };
    Ok(res)
}

pub fn get_minter_env<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    env: Env,
) -> StdResult<Env> {
    let mut minter_env = Env {
        message: env.message.clone(),
        contract: env.contract.clone(),
        block: env.block.clone(),
    };

    let config = token_info_read(&deps.storage).load()?;
    minter_env.message.sender = deps
        .api
        .human_address(&config.mint.unwrap().minter)
        .unwrap();

    Ok(minter_env)
}
