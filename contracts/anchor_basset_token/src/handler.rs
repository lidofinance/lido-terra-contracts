use cosmwasm_std::{
    to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HandleResult, HumanAddr,
    Querier, Storage, Uint128, WasmMsg,
};

use crate::querier::query_reward_contract;
use cw20_base::allowances::{
    handle_burn_from as cw20_burn_from, handle_send_from as cw20_send_from,
    handle_transfer_from as cw20_transfer_from,
};
use cw20_base::contract::{
    handle_burn as cw20_burn, handle_mint as cw20_mint, handle_send as cw20_send,
    handle_transfer as cw20_transfer,
};
use reward_querier::HandleMsg::{DecreaseBalance, IncreaseBalance};

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
