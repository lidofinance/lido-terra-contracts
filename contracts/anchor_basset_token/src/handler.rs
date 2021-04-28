use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HandleResult, HumanAddr, Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::msg::Cw20HookMsg;
use crate::querier::{query_exchange_rates, query_reward_contract, query_stluna_contract};
use anchor_basset_reward::msg::HandleMsg::{DecreaseBalance, IncreaseBalance};
use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use cw20_base::allowances::{
    handle_burn_from as cw20_burn_from, handle_send_from as cw20_send_from,
    handle_transfer_from as cw20_transfer_from,
};
use cw20_base::contract::{
    handle_burn as cw20_burn, handle_mint as cw20_mint, handle_send as cw20_send,
    handle_transfer as cw20_transfer,
};
use std::ops::Mul;
use cw20_base::state::token_info_read;

pub fn handle_transfer<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let sender = env.message.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse = cw20_transfer(deps, env, recipient.clone(), amount)?;
    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.clone(),
                msg: to_binary(&DecreaseBalance {
                    address: sender,
                    amount,
                })
                    .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract,
                msg: to_binary(&IncreaseBalance {
                    address: recipient,
                    amount,
                })
                    .unwrap(),
                send: vec![],
            }),
        ],
        log: res.log,
        data: None,
    })
}

pub fn handle_burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
) -> HandleResult {
    let sender = env.message.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse = cw20_burn(deps, env, amount)?;
    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract,
            msg: to_binary(&DecreaseBalance {
                address: sender,
                amount,
            })
                .unwrap(),
            send: vec![],
        })],
        log: res.log,
        data: None,
    })
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse = cw20_mint(deps, env, recipient.clone(), amount)?;
    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract,
            msg: to_binary(&IncreaseBalance {
                address: recipient,
                amount,
            })
                .unwrap(),
            send: vec![],
        })],
        log: res.log,
        data: None,
    })
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    let sender = env.message.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse = cw20_send(deps, env, contract.clone(), amount, msg)?;
    Ok(HandleResponse {
        messages: vec![
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.clone(),
                    msg: to_binary(&DecreaseBalance {
                        address: sender,
                        amount,
                    })
                        .unwrap(),
                    send: vec![],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract,
                    msg: to_binary(&IncreaseBalance {
                        address: contract,
                        amount,
                    })
                        .unwrap(),
                    send: vec![],
                }),
            ],
            res.messages,
        ]
            .concat(),
        log: res.log,
        data: None,
    })
}

pub fn handle_transfer_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    recipient: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse =
        cw20_transfer_from(deps, env, owner.clone(), recipient.clone(), amount)?;
    Ok(HandleResponse {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.clone(),
                msg: to_binary(&DecreaseBalance {
                    address: owner,
                    amount,
                })
                    .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract,
                msg: to_binary(&IncreaseBalance {
                    address: recipient,
                    amount,
                })
                    .unwrap(),
                send: vec![],
            }),
        ],
        log: res.log,
        data: None,
    })
}

pub fn handle_burn_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse = cw20_burn_from(deps, env, owner.clone(), amount)?;
    Ok(HandleResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract,
            msg: to_binary(&DecreaseBalance {
                address: owner,
                amount,
            })
                .unwrap(),
            send: vec![],
        })],
        log: res.log,
        data: None,
    })
}

pub fn handle_send_from<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: HumanAddr,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> HandleResult {
    let reward_contract = query_reward_contract(&deps)?;

    let res: HandleResponse =
        cw20_send_from(deps, env, owner.clone(), contract.clone(), amount, msg)?;
    Ok(HandleResponse {
        messages: vec![
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.clone(),
                    msg: to_binary(&DecreaseBalance {
                        address: owner,
                        amount,
                    })
                        .unwrap(),
                    send: vec![],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract,
                    msg: to_binary(&IncreaseBalance {
                        address: contract,
                        amount,
                    })
                        .unwrap(),
                    send: vec![],
                }),
            ],
            res.messages,
        ]
            .concat(),
        log: res.log,
        data: None,
    })
}

/// CW20 token receive handler.
pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<HandleResponse> {
    let contract_addr = env.message.sender.clone();

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::Convert {} => {
                let stluna_contract = query_stluna_contract(deps)?;

                // only token contract can execute this message
                if contract_addr != stluna_contract {
                    Err(StdError::unauthorized())
                } else {
                    handle_convert_stluna(deps, env, cw20_msg.amount, cw20_msg.sender, stluna_contract)
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

fn handle_convert_stluna<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
    stluna_contract: HumanAddr,
) -> StdResult<HandleResponse> {
    let (bluna_exchange_rate, stluna_exchange_rate) = query_exchange_rates(deps)?;

    let bluna_equivalent = bluna_exchange_rate.mul(amount);
    let stluna_equivalent = stluna_exchange_rate.mul(amount);

    let value_ratio = Decimal::from_ratio(bluna_equivalent, stluna_equivalent);
    let bluna_to_mint = value_ratio.mul(amount);

    let mut messages: Vec<CosmosMsg> = vec![];
    let burn_msg = Cw20HandleMsg::Burn { amount };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: stluna_contract,
        msg: to_binary(&burn_msg)?,
        send: vec![],
    }));

    handle_mint(deps, get_minter_env(deps, env)?, sender.clone(), bluna_to_mint)?;

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "convert_stluna"),
            log("from", sender),
            log("bluna_exchange_rate", bluna_exchange_rate),
            log("stluna_exchange_rate", stluna_exchange_rate),
            log("stluna_amount", amount),
            log("bluna_amount", bluna_to_mint),
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
        .human_address(&config.mint.unwrap().minter).unwrap();

    Ok(minter_env)
}
