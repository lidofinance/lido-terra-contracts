use anchor_basset_gov::contract::{handle, init};
use anchor_basset_gov::msg::InitMsg;
use anchor_basset_gov::state::POOL_INFO;
use anchor_basset_reward::contracts::{
    handle as reward_handle, init as reward_init, query as reward_query,
};
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::msg::HandleMsg::{
    ClaimRewards, SwapToRewardDenom, UpdateGlobalIndex, UpdateParams as UpdateRewardParams,
    UpdateUserIndex,
};
use anchor_basset_reward::msg::QueryMsg::{GlobalIndex, PendingRewards, UserIndex};
use anchor_basset_reward::state::Config;
use anchor_basset_token::contract::{handle as token_handle, init as token_init};
use anchor_basset_token::msg::HandleMsg::{Burn, Mint, Send, Transfer};
use anchor_basset_token::msg::TokenInitMsg;
use anchor_basset_token::state::MinterData;
use anchor_basset_token::state::TokenInfo as TokenConfig;
use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    coin, from_binary, to_binary, Api, BankMsg, CanonicalAddr, CosmosMsg, Decimal, Extern,
    HumanAddr, Querier, StakingMsg, StdResult, Storage, Uint128, Validator, WasmMsg,
};
use cosmwasm_storage::Singleton;
use cw20::MinterResponse;
use gov_courier::Registration::{Reward, Token};
use gov_courier::{Cw20HookMsg, HandleMsg, PoolInfo};

mod common;
use common::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
use gov_courier::HandleMsg::UpdateParams;

const TOKEN_INFO_KEY: &[u8] = b"token_info";
const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2";
pub static CONFIG: &[u8] = b"config";

pub fn init_all<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    owner: HumanAddr,
    reward_contract: HumanAddr,
    token_contract: HumanAddr,
) {
    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        reward_code_id: 0,
        token_code_id: 0,
    };

    let env = mock_env(owner.clone(), &[]);
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    let gov_address = deps
        .api
        .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
        .unwrap();

    let gov_env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
    let reward_in = default_reward(gov_address.clone());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(gov_address.clone(), owner);
    token_init(&mut deps, gov_env, token_int).unwrap();

    let register_msg = HandleMsg::RegisterSubcontracts { contract: Reward };
    let register_env = mock_env(reward_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubcontracts { contract: Token };
    let register_env = mock_env(token_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    set_reward_config(&mut deps.storage, gov_address.clone()).unwrap();
    set_token_info(&mut deps.storage, gov_address).unwrap();
}

pub fn do_bond<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr: HumanAddr,
    amount: Uint128,
    validator: Validator,
    twice: bool,
) {
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bond = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&addr, &[coin(amount.0, "uluna")]);
    let _res = handle(&mut deps, env, bond);
    let msg = Mint {
        recipient: addr.clone(),
        amount,
    };

    let owner = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let env = mock_env(&owner, &[]);
    let res = token_handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
    let message = &res.messages[0];
    if !twice {
        match message {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                send: _,
            }) => {
                assert_eq!(contract_addr, &HumanAddr::from("reward"));
                assert_eq!(
                    msg,
                    &to_binary(&UpdateUserIndex {
                        address: addr.clone(),
                        is_send: None
                    })
                    .unwrap()
                )
            }
            _ => panic!("Unexpected message: {:?}", message),
        }
    }
    if twice {
        match message {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                send: _,
            }) => {
                assert_eq!(contract_addr, &HumanAddr::from("reward"));
                assert_eq!(
                    msg,
                    &to_binary(&UpdateUserIndex {
                        address: addr,
                        is_send: Some(Uint128(10))
                    })
                    .unwrap()
                )
            }
            _ => panic!("Unexpected message: {:?}", message),
        }
    }
}

pub fn do_update_global<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    expected_res: &str,
) {
    let reward_msg = HandleMsg::UpdateGlobalIndex {};

    let env = mock_env(&HumanAddr::from("owner1"), &[]);
    let res = handle(deps, env, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    reward_update_global(deps, expected_res);
}

pub fn reward_update_global<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    expected_res: &str,
) {
    let owner = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let mut env = mock_env(&owner, &[]);
    env.contract.address = HumanAddr::from("reward");

    let update_global_index = UpdateGlobalIndex {};
    let reward_update = reward_handle(deps, env, update_global_index).unwrap();
    assert_eq!(reward_update.messages.len(), 0);

    let query = GlobalIndex {};
    let qry = reward_query(&deps, query).unwrap();
    let res: Decimal = from_binary(&qry).unwrap();
    assert_eq!(res.to_string(), expected_res);
}

fn sample_validator<U: Into<HumanAddr>>(addr: U) -> Validator {
    Validator {
        address: addr.into(),
        commission: Decimal::percent(3),
        max_commission: Decimal::percent(10),
        max_change_rate: Decimal::percent(1),
    }
}

fn set_validator_mock(querier: &mut WasmMockQuerier) {
    querier.update_staking(
        "uluna",
        &[
            sample_validator(DEFAULT_VALIDATOR),
            sample_validator(DEFAULT_VALIDATOR2),
        ],
        &[],
    );
}

fn default_reward(owner: CanonicalAddr) -> RewardInitMsg {
    RewardInitMsg {
        owner,
        init_hook: None,
    }
}

pub fn set_pool_info<S: Storage>(
    storage: &mut S,
    ex_rate: Decimal,
    total_boned: Uint128,
    reward_account: CanonicalAddr,
    token_account: CanonicalAddr,
) -> StdResult<()> {
    Singleton::new(storage, POOL_INFO).save(&PoolInfo {
        exchange_rate: ex_rate,
        total_bond_amount: total_boned,
        last_index_modification: 0,
        reward_account,
        is_reward_exist: true,
        is_token_exist: true,
        token_account,
    })
}

pub fn set_params<S: Storage, A: Api, Q: Querier>(mut deps: &mut Extern<S, A, Q>) {
    let update_prams = UpdateParams {
        epoch_time: 30,
        coin_denom: "uluna".to_string(),
        undelegated_epoch: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        swap_denom: Some("uusd".to_string()),
    };
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 1);

    let reward_params = UpdateRewardParams {
        swap_denom: "uusd".to_string(),
    };

    let owner = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
    let reward_res = reward_handle(&mut deps, owner, reward_params).unwrap();
    assert_eq!(reward_res.messages.len(), 0);
}

pub fn set_reward_config<S: Storage>(storage: &mut S, owner: CanonicalAddr) -> StdResult<()> {
    Singleton::new(storage, CONFIG).save(&Config { owner })
}

pub fn set_token_info<S: Storage>(storage: &mut S, owner: CanonicalAddr) -> StdResult<()> {
    Singleton::new(storage, TOKEN_INFO_KEY).save(&TokenConfig {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        total_supply: Default::default(),
        mint: Some(MinterData {
            minter: owner.clone(),
            cap: None,
        }),
        owner,
    })
}

fn default_token(owner: CanonicalAddr, minter: HumanAddr) -> TokenInitMsg {
    TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse { minter, cap: None }),
        init_hook: None,
        owner,
    }
}

pub fn do_update_user_in<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr1: HumanAddr,
    amount: Uint128,
    twice: bool,
) {
    if !twice {
        let update_user_index = UpdateUserIndex {
            address: addr1,
            is_send: None,
        };
        let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
        let gov_env = mock_env(gov, &[]);
        let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
        assert_eq!(res.messages.len(), 0);
    } else {
        let update_user_index = UpdateUserIndex {
            address: addr1,
            is_send: Some(amount),
        };
        let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
        let gov_env = mock_env(gov, &[]);
        let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
        assert_eq!(res.messages.len(), 0);
    }
}

//this will check the update global index workflow
#[test]
fn send_update_global_index() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract,
    );
    set_params(&mut deps);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let withdraw = &res.messages[0];
    match withdraw {
        CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient,
        }) => {
            assert_eq!(val, &validator.address);
            assert_eq!(recipient.is_none(), true);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let swap = &res.messages[1];
    match swap {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(msg, &to_binary(&SwapToRewardDenom {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", swap),
    }

    let update_g_index = &res.messages[2];
    match update_g_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(msg, &to_binary(&UpdateGlobalIndex {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", update_g_index),
    }
    reward_update_global(&mut deps, "200");
}

#[test]
pub fn proper_update_user_index() {
    let mut deps = dependencies(20, &[]);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let val = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    init_all(&mut deps, owner, reward_contract, token_contract);
    set_params(&mut deps);
    let addr1 = HumanAddr::from("addr0001");

    //first bond
    do_bond(&mut deps, addr1.clone(), Uint128(10), val.clone(), false);
    let update_user_index = UpdateUserIndex {
        address: addr1.clone(),
        is_send: None,
    };
    let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let gov_env = mock_env(gov, &[]);
    let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
    assert_eq!(res.messages.len(), 0);

    let query_index = UserIndex {
        address: addr1.clone(),
    };
    let query_res = reward_query(&deps, query_index).unwrap();
    let index: Decimal = from_binary(&query_res).unwrap();
    assert_eq!(index.to_string(), "0");

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    //update global_index
    do_update_global(&mut deps, "200");

    //second bond
    do_bond(&mut deps, addr1.clone(), Uint128(10), val, true);

    //send update user index
    let update_user_index = UpdateUserIndex {
        address: addr1.clone(),
        is_send: Some(Uint128(10)),
    };
    let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let gov_env = mock_env(gov, &[]);
    let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
    assert_eq!(res.messages.len(), 0);

    //get the index of the user
    let query_index = UserIndex {
        address: addr1.clone(),
    };
    let query_res = reward_query(&deps, query_index).unwrap();
    let index: Decimal = from_binary(&query_res).unwrap();
    assert_eq!(index.to_string(), "200");

    //get the pending reward of the user
    let query_pending = PendingRewards { address: addr1 };
    let query_res = reward_query(&deps, query_pending).unwrap();
    let index: Uint128 = from_binary(&query_res).unwrap();
    assert_eq!(index, Uint128(2000));
}

#[test]
pub fn integrated_transfer() {
    let mut deps = dependencies(20, &[]);

    //add tax
    deps.querier._with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    let addr1 = HumanAddr::from("addr0001");
    let addr2 = HumanAddr::from("addr0002");
    let amount1 = Uint128::from(10u128);
    let transfer = Uint128::from(1u128);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let val = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    init_all(&mut deps, owner, reward_contract, token_contract);
    set_params(&mut deps);

    //bond first
    do_bond(&mut deps, addr1.clone(), amount1, val.clone(), false);
    do_bond(&mut deps, addr2.clone(), amount1, val.clone(), false);

    //update user index
    do_update_user_in(&mut deps, addr1.clone(), amount1, false);
    do_update_user_in(&mut deps, addr2.clone(), amount1, false);

    //set addr1's balance to 10 in token contract
    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[(&addr1, &Uint128(10u128)), (&addr2, &Uint128(10u128))],
    )]);

    //update global_index
    do_update_global(&mut deps, "100");

    //bond first
    do_bond(&mut deps, addr1.clone(), amount1, val, true);

    let env = mock_env(addr1.clone(), &[]);
    let msg = Transfer {
        recipient: addr2.clone(),
        amount: transfer,
    };
    let res = token_handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    let update_user_index = &res.messages[1];
    match update_user_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: _,
            msg,
            send: _,
        }) => {
            assert_eq!(
                msg,
                &to_binary(&UpdateUserIndex {
                    address: addr2.clone(),
                    is_send: Some(Uint128(10))
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}",),
    }

    let claim = ClaimRewards {
        recipient: Some(addr1.clone()),
    };

    let mut env = mock_env(HumanAddr::from("token"), &[]);
    env.contract.address = HumanAddr::from("reward");
    let res = reward_handle(&mut deps, env, claim).unwrap();
    assert_eq!(res.messages.len(), 1);

    let send = &res.messages[0];
    match send {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address, &HumanAddr::from("reward"));
            assert_eq!(to_address, &addr1);
            //the tax is 1 percent there fore 1000 - 10 = 990
            assert_eq!(amount.get(0).unwrap().amount, Uint128(990));
        }
        _ => panic!("Unexpected message: {:?}", send),
    }

    //get the index of the user
    let query_index = UserIndex { address: addr1 };
    let query_res = reward_query(&deps, query_index).unwrap();
    let index: Decimal = from_binary(&query_res).unwrap();
    assert_eq!(index.to_string(), "100");

    //send update user index
    let update_user_index = UpdateUserIndex {
        address: addr2.clone(),
        is_send: Some(Uint128(10)),
    };
    let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let gov_env = mock_env(gov, &[]);
    let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
    assert_eq!(res.messages.len(), 0);

    //get the index of the user
    let query_index = UserIndex {
        address: addr2.clone(),
    };
    let query_res = reward_query(&deps, query_index).unwrap();
    let index: Decimal = from_binary(&query_res).unwrap();
    assert_eq!(index.to_string(), "100");

    //get the pending reward of the user
    let query_pending = PendingRewards { address: addr2 };
    let query_res = reward_query(&deps, query_pending).unwrap();
    let index: Uint128 = from_binary(&query_res).unwrap();
    assert_eq!(index, Uint128(1000));
}

#[test]
pub fn integrated_send() {
    let mut deps = dependencies(20, &[]);

    //add tax
    deps.querier._with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    let addr1 = HumanAddr::from("addr0001");
    let contract = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let amount1 = Uint128::from(10u128);
    let transfer = Uint128::from(1u128);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let val = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    init_all(&mut deps, owner, reward_contract, token_contract);
    set_params(&mut deps);

    //bond first
    do_bond(&mut deps, addr1.clone(), amount1, val.clone(), false);
    do_bond(&mut deps, contract.clone(), amount1, val.clone(), false);

    //update user index
    do_update_user_in(&mut deps, addr1.clone(), amount1, false);
    do_update_user_in(&mut deps, contract.clone(), amount1, false);

    //set addr1's balance to 10 in token contract
    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[(&addr1, &Uint128(10u128)), (&contract, &Uint128(10u128))],
    )]);

    //update global_index
    do_update_global(&mut deps, "100");

    //bond first
    do_bond(&mut deps, addr1.clone(), amount1, val, true);

    let env = mock_env(addr1.clone(), &[]);
    let send_msg = Send {
        contract: contract.clone(),
        amount: transfer,
        msg: Some(to_binary(&Cw20HookMsg::Unbond {}).unwrap()),
    };
    let res = token_handle(&mut deps, env, send_msg).unwrap();
    assert_eq!(res.messages.len(), 3);

    let claim = ClaimRewards {
        recipient: Some(addr1.clone()),
    };

    let mut env = mock_env(HumanAddr::from("token"), &[]);
    env.contract.address = HumanAddr::from("reward");
    let res = reward_handle(&mut deps, env, claim).unwrap();
    assert_eq!(res.messages.len(), 1);

    let send = &res.messages[0];
    match send {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address, &HumanAddr::from("reward"));
            assert_eq!(to_address, &addr1);
            //the tax is 1 percent there fore 1000 - 10 = 990
            assert_eq!(amount.get(0).unwrap().amount, Uint128(990));
        }
        _ => panic!("Unexpected message: {:?}", send),
    }

    //send update user index
    let update_user_index = UpdateUserIndex {
        address: contract.clone(),
        is_send: Some(Uint128(10)),
    };
    let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let gov_env = mock_env(gov, &[]);
    let res = reward_handle(&mut deps, gov_env, update_user_index).unwrap();
    assert_eq!(res.messages.len(), 0);

    //get the index of the user
    let query_index = UserIndex {
        address: contract.clone(),
    };
    let query_res = reward_query(&deps, query_index).unwrap();
    let index: Decimal = from_binary(&query_res).unwrap();
    assert_eq!(index.to_string(), "100");

    //get the pending reward of the user
    let query_pending = PendingRewards { address: contract };
    let query_res = reward_query(&deps, query_pending).unwrap();
    let index: Uint128 = from_binary(&query_res).unwrap();
    assert_eq!(index, Uint128(1000));
}

#[test]
pub fn integrated_burn() {
    let mut deps = dependencies(20, &[]);

    //add tax
    deps.querier._with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    let contract = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let amount1 = Uint128::from(10u128);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let val = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    init_all(&mut deps, owner, reward_contract, token_contract);
    set_params(&mut deps);

    //bond first
    do_bond(&mut deps, contract.clone(), amount1, val.clone(), false);

    //update user index
    do_update_user_in(&mut deps, contract.clone(), amount1, false);

    //set addr1's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&contract, &Uint128(10u128))])]);

    //update global_index
    do_update_global(&mut deps, "200");

    //bond first
    do_bond(&mut deps, contract.clone(), amount1, val, true);

    let env = mock_env(contract.clone(), &[]);
    let burn = Burn { amount: amount1 };
    let res = token_handle(&mut deps, env, burn).unwrap();
    assert_eq!(res.messages.len(), 1);

    let claim = ClaimRewards {
        recipient: Some(contract.clone()),
    };

    let mut env = mock_env(HumanAddr::from("token"), &[]);
    env.contract.address = HumanAddr::from("reward");
    let res = reward_handle(&mut deps, env, claim).unwrap();
    assert_eq!(res.messages.len(), 1);

    let send = &res.messages[0];
    match send {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address, &HumanAddr::from("reward"));
            assert_eq!(to_address, &contract);
            //the tax is 1 percent there fore 2000 - 20 = 1980
            assert_eq!(amount.get(0).unwrap().amount, Uint128(1980));
        }
        _ => panic!("Unexpected message: {:?}", send),
    }
}
