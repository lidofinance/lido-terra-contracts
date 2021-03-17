use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{
    coins, to_binary, Api, CosmosMsg, Extern, HumanAddr, Querier, Storage, Uint128, WasmMsg,
};

use cw20::{Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};
use cw20_base::contract::{query_minter, query_token_info};
use cw20_base::msg::HandleMsg;
use reward_querier::HandleMsg::{DecreaseBalance, IncreaseBalance};

use crate::contract::{handle, init};
use crate::msg::TokenInitMsg;
use crate::state::read_hub_contract;
use crate::testing::mock_querier::{
    mock_dependencies, MOCK_HUB_CONTRACT_ADDR, MOCK_REWARD_CONTRACT_ADDR,
};

const CANONICAL_LENGTH: usize = 20;

// this will set up the init for other tests
fn do_init_with_minter<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    minter: &HumanAddr,
    cap: Option<Uint128>,
) -> TokenInfoResponse {
    _do_init(
        deps,
        Some(MinterResponse {
            minter: minter.into(),
            cap,
        }),
    )
}

// this will set up the init for other tests
fn _do_init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    mint: Option<MinterResponse>,
) -> TokenInfoResponse {
    let hub_contract = HumanAddr::from(MOCK_HUB_CONTRACT_ADDR);
    let init_msg = TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: mint.clone(),
        hub_contract,
    };

    let env = mock_env(&HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), &[]);
    let res = init(deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let meta = query_token_info(&deps).unwrap();
    assert_eq!(
        meta,
        TokenInfoResponse {
            name: "bluna".to_string(),
            symbol: "BLUNA".to_string(),
            decimals: 6,
            total_supply: Uint128::zero(),
        }
    );
    assert_eq!(query_minter(&deps).unwrap(), mint,);
    meta
}

pub fn do_mint<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr: HumanAddr,
    amount: Uint128,
) {
    let msg = HandleMsg::Mint {
        recipient: addr,
        amount,
    };
    let owner = HumanAddr::from(MOCK_HUB_CONTRACT_ADDR);
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(CANONICAL_LENGTH, &[]);
    let hub_contract = HumanAddr::from(MOCK_HUB_CONTRACT_ADDR);
    let hub_contract_raw = deps.api.canonical_address(&hub_contract).unwrap();

    let init_msg = TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: None,
        hub_contract: hub_contract.clone(),
    };
    let env = mock_env(&hub_contract, &[]);
    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    assert_eq!(
        query_token_info(&deps).unwrap(),
        TokenInfoResponse {
            name: "bluna".to_string(),
            symbol: "BLUNA".to_string(),
            decimals: 6,
            total_supply: Uint128::zero(),
        }
    );

    assert_eq!(
        query_minter(&deps).unwrap(),
        Some(MinterResponse {
            minter: hub_contract,
            cap: None
        })
    );

    assert_eq!(read_hub_contract(&deps.storage).unwrap(), hub_contract_raw);
}

#[test]
fn transfer() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let addr2 = HumanAddr::from("addr0002");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr1.clone(), amount1);

    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Transfer {
        recipient: addr2.clone(),
        amount: Uint128(1u128),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&DecreaseBalance {
                    address: addr1,
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&IncreaseBalance {
                    address: addr2,
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

#[test]
fn transfer_from() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let addr2 = HumanAddr::from("addr0002");
    let addr3 = HumanAddr::from("addr0003");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr1.clone(), amount1);

    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: addr3.clone(),
        amount: Uint128(1u128),
        expires: None,
    };
    let _ = handle(&mut deps, env, msg).unwrap();

    let env = mock_env(addr3, &[]);
    let msg = HandleMsg::TransferFrom {
        owner: addr1.clone(),
        recipient: addr2.clone(),
        amount: Uint128(1u128),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&DecreaseBalance {
                    address: addr1,
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&IncreaseBalance {
                    address: addr2,
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
}

#[test]
fn mint() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr = HumanAddr::from("addr0000");

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);

    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::Mint {
        recipient: addr.clone(),
        amount: Uint128(1u128),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
            msg: to_binary(&IncreaseBalance {
                address: addr,
                amount: Uint128(1u128),
            })
            .unwrap(),
            send: vec![],
        }),]
    );
}

#[test]
fn burn() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr = HumanAddr::from("addr0000");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr.clone(), amount1);

    let env = mock_env(addr.clone(), &[]);
    let msg = HandleMsg::Burn {
        amount: Uint128(1u128),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
            msg: to_binary(&DecreaseBalance {
                address: addr,
                amount: Uint128(1u128),
            })
            .unwrap(),
            send: vec![],
        }),]
    );
}

#[test]
fn burn_from() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr = HumanAddr::from("addr0000");
    let addr1 = HumanAddr::from("addr0001");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr.clone(), amount1);

    let env = mock_env(addr.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: addr1.clone(),
        amount: Uint128(1u128),
        expires: None,
    };
    let _ = handle(&mut deps, env, msg).unwrap();

    let env = mock_env(addr1, &[]);
    let msg = HandleMsg::BurnFrom {
        owner: addr.clone(),
        amount: Uint128(1u128),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
            msg: to_binary(&DecreaseBalance {
                address: addr,
                amount: Uint128(1u128),
            })
            .unwrap(),
            send: vec![],
        }),]
    );
}

#[test]
fn send() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let dummny_contract_addr = HumanAddr::from("dummy");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr1.clone(), amount1);

    let dummy_msg = HandleMsg::Transfer {
        recipient: addr1.clone(),
        amount: Uint128(1u128),
    };

    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Send {
        contract: dummny_contract_addr.clone(),
        amount: Uint128(1u128),
        msg: Some(to_binary(&dummy_msg).unwrap()),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(
        res.messages[0..2].to_vec(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&DecreaseBalance {
                    address: addr1.clone(),
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&IncreaseBalance {
                    address: dummny_contract_addr.clone(),
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );
    assert_eq!(
        res.messages[2],
        Cw20ReceiveMsg {
            sender: addr1,
            amount: Uint128(1),
            msg: Some(to_binary(&dummy_msg).unwrap()),
        }
        .into_cosmos_msg(dummny_contract_addr)
        .unwrap()
    );
}

#[test]
fn send_from() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let addr2 = HumanAddr::from("addr0002");
    let dummny_contract_addr = HumanAddr::from("dummy");
    let amount1 = Uint128::from(12340000u128);

    do_init_with_minter(&mut deps, &HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), None);
    do_mint(&mut deps, addr1.clone(), amount1);

    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: addr2.clone(),
        amount: Uint128(1u128),
        expires: None,
    };
    let _ = handle(&mut deps, env, msg).unwrap();

    let dummy_msg = HandleMsg::Transfer {
        recipient: addr1.clone(),
        amount: Uint128(1u128),
    };

    let env = mock_env(addr2.clone(), &[]);
    let msg = HandleMsg::SendFrom {
        owner: addr1.clone(),
        contract: dummny_contract_addr.clone(),
        amount: Uint128(1u128),
        msg: Some(to_binary(&dummy_msg).unwrap()),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(
        res.messages[0..2].to_vec(),
        vec![
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&DecreaseBalance {
                    address: addr1,
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR),
                msg: to_binary(&IncreaseBalance {
                    address: dummny_contract_addr.clone(),
                    amount: Uint128(1u128),
                })
                .unwrap(),
                send: vec![],
            }),
        ]
    );

    assert_eq!(
        res.messages[2],
        Cw20ReceiveMsg {
            sender: addr2,
            amount: Uint128(1),
            msg: Some(to_binary(&dummy_msg).unwrap()),
        }
        .into_cosmos_msg(dummny_contract_addr)
        .unwrap()
    );
}
