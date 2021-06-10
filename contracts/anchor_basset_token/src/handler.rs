use cosmwasm_std::{
    to_binary, Binary, CosmosMsg, DepsMut, Env, MessageInfo, Response, Uint128, WasmMsg,
};

use crate::querier::query_reward_contract;
use basset::reward::ExecuteMsg::{DecreaseBalance, IncreaseBalance};
use cw20_base::allowances::{
    execute_burn_from as cw20_burn_from, execute_send_from as cw20_send_from,
    execute_transfer_from as cw20_transfer_from,
};
use cw20_base::contract::{
    execute_burn as cw20_burn, execute_mint as cw20_mint, execute_send as cw20_send,
    execute_transfer as cw20_transfer,
};
use cw20_base::ContractError;

pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let sender = info.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    let res: Response = cw20_transfer(deps, env, info, recipient, amount)?;
    Ok(Response {
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.to_string(),
                msg: to_binary(&DecreaseBalance {
                    address: sender.to_string(),
                    amount,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.to_string(),
                msg: to_binary(&IncreaseBalance {
                    address: rcpt_addr.to_string(),
                    amount,
                })
                .unwrap(),
                send: vec![],
            }),
        ],
        attributes: res.attributes,
        submessages: vec![],
        data: None,
    })
}

pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let sender = info.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let res: Response = cw20_burn(deps, env, info, amount)?;
    Ok(Response {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract.to_string(),
            msg: to_binary(&DecreaseBalance {
                address: sender.to_string(),
                amount,
            })
            .unwrap(),
            send: vec![],
        })],
        attributes: res.attributes,
        submessages: vec![],
        data: None,
    })
}

pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let reward_contract = query_reward_contract(&deps)?;

    let res: Response = cw20_mint(deps, env, info, recipient.clone(), amount)?;
    Ok(Response {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract.to_string(),
            msg: to_binary(&IncreaseBalance {
                address: recipient,
                amount,
            })
            .unwrap(),
            send: vec![],
        })],
        attributes: res.attributes,
        submessages: vec![],
        data: None,
    })
}

pub fn execute_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let sender = info.sender.clone();
    let reward_contract = query_reward_contract(&deps)?;

    let res: Response = cw20_send(deps, env, info, contract.clone(), amount, msg)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.to_string(),
                    msg: to_binary(&DecreaseBalance {
                        address: sender.to_string(),
                        amount,
                    })
                    .unwrap(),
                    send: vec![],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.to_string(),
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
        data: None,
        attributes: res.attributes,
    })
}

pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let reward_contract = query_reward_contract(&deps)?;

    let valid_owner = deps.api.addr_validate(owner.as_str())?;

    let res: Response = cw20_transfer_from(deps, env, info, owner, recipient.clone(), amount)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.to_string(),
                msg: to_binary(&DecreaseBalance {
                    address: valid_owner.to_string(),
                    amount,
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: reward_contract.to_string(),
                msg: to_binary(&IncreaseBalance {
                    address: recipient,
                    amount,
                })
                .unwrap(),
                send: vec![],
            }),
        ],
        data: None,
        attributes: res.attributes,
    })
}

pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let reward_contract = query_reward_contract(&deps)?;

    let valid_owner = deps.api.addr_validate(owner.as_str())?;

    let res: Response = cw20_burn_from(deps, env, info, owner, amount)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract.to_string(),
            msg: to_binary(&DecreaseBalance {
                address: valid_owner.to_string(),
                amount,
            })
            .unwrap(),
            send: vec![],
        })],
        data: None,
        attributes: res.attributes,
    })
}

pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let reward_contract = query_reward_contract(&deps)?;

    let valid_owner = deps.api.addr_validate(owner.as_str())?;

    let res: Response = cw20_send_from(deps, env, info, owner, contract.clone(), amount, msg)?;
    Ok(Response {
        submessages: vec![],
        messages: vec![
            vec![
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.to_string(),
                    msg: to_binary(&DecreaseBalance {
                        address: valid_owner.to_string(),
                        amount,
                    })
                    .unwrap(),
                    send: vec![],
                }),
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: reward_contract.to_string(),
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
        data: None,
        attributes: res.attributes,
    })
}
