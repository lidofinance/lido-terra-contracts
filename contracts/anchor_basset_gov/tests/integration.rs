//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
//! You can easily convert unit tests to integration tests as follows:
//! 1. Copy them over verbatim
//! 2. Then change
//!      let mut deps = mock_dependencies(20, &[]);
//!    to
//!      let mut deps = mock_instance(WASM, &[]);
//! 3. If you access raw storage, where ever you see something like:
//!      deps.storage.get(CONFIG_KEY).expect("no data stored");
//!    replace it with:
//!      deps.with_storage(|store| {
//!          let data = store.get(CONFIG_KEY).expect("no data stored");
//!          //...
//!      });
//! 4. Anywhere you see query(&deps, ...) you must replace it with query(&mut deps, ...)
use cosmwasm_std::{
    coin, from_binary, to_binary, Api, BankMsg, CanonicalAddr, Coin, CosmosMsg, Decimal, Extern,
    FullDelegation, HumanAddr, InitResponse, Querier, StakingMsg, StdError, StdResult, Storage,
    Uint128, Validator, WasmMsg,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env};

use anchor_bluna::msg::InitMsg;
use gov_courier::{HandleMsg, PoolInfo};

use anchor_bluna::contract::{handle, handle_burn, init, query};

use anchor_basset_reward::contracts::init as reward_init;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::state::Config;
use anchor_basset_token::contract::{
    handle as token_handle, init as token_init, query as token_query,
};
use anchor_basset_token::msg::HandleMsg::{Burn, Mint, Send};
use anchor_basset_token::msg::QueryMsg::{Balance, TokenInfo};
use anchor_basset_token::msg::TokenInitMsg;
use anchor_basset_token::state::{MinterData, TokenInfo as TokenConfig};
use cosmwasm_storage::Singleton;
use cw20::{BalanceResponse, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};
use gov_courier::Cw20HookMsg::InitBurn;
use gov_courier::HandleMsg::{Receive, ReportSlashing};
use gov_courier::Registration::{Reward, Token};

mod common;
use anchor_basset_reward::msg::HandleMsg::ClaimReward;
use anchor_bluna::msg::QueryMsg::ExchangeRate;
use common::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2";
pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub static POOL_INFO: &[u8] = b"pool_info";
pub static CONFIG: &[u8] = b"config";
const TOKEN_INFO_KEY: &[u8] = b"token_info";

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

fn default_reward(owner: CanonicalAddr) -> RewardInitMsg {
    RewardInitMsg {
        owner,
        init_hook: None,
    }
}

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

    let gov_address = deps
        .api
        .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
        .unwrap();

    let gov_env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
    let env = mock_env(owner.clone(), &[]);
    init(&mut deps, env, msg).unwrap();

    let reward_in = default_reward(gov_address.clone());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(gov_address.clone(), owner);
    token_init(&mut deps, gov_env, token_int).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Reward };
    let register_env = mock_env(reward_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Token };
    let register_env = mock_env(token_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    set_reward_config(&mut deps.storage, gov_address.clone()).unwrap();
    set_token_info(&mut deps.storage, gov_address).unwrap();
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        reward_code_id: 0,
        token_code_id: 0,
    };

    let gov_address = deps
        .api
        .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
        .unwrap();
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);

    let owner = HumanAddr::from("addr0000");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();

    let env = mock_env(owner, &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(2, res.messages.len());

    let reward_in = default_reward(gov_address.clone());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(gov_address, HumanAddr::from(MOCK_CONTRACT_ADDR));
    token_init(&mut deps, gov_env, token_int).unwrap();
    set_token_info(&mut deps.storage, owner_raw).unwrap();

    let other_contract = HumanAddr::from("other_contract");
    let register_msg = HandleMsg::RegisterSubContracts { contract: Reward };
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(1, exec.messages.len());

    let token_contract = HumanAddr::from("token_contract");
    let register_msg = HandleMsg::RegisterSubContracts { contract: Token };
    let register_env = mock_env(&token_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    //check token_info
    let token_inf = TokenInfo {};
    let query_result = token_query(&deps, token_inf).unwrap();
    let value: TokenInfoResponse = from_binary(&query_result).unwrap();
    assert_eq!("bluna".to_string(), value.name);
    assert_eq!("BLUNA".to_string(), value.symbol);
    assert_eq!(Uint128::zero(), value.total_supply);
    assert_eq!(6, value.decimals);
}

#[test]
fn proper_mint() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };
    let address = env.contract.address;
    let gov_env = mock_env(address, &[]);

    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    set_delegation(&mut deps.querier, 10, "uluna");
    //check the balance of the bob
    let balance_msg = Balance { address: bob };
    let query_result = token_query(&deps, balance_msg).unwrap();
    let value: BalanceResponse = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(10), value.balance);
}

#[test]
fn proper_deregister() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let msg = HandleMsg::RegisterValidator {
        validator: validator2.address,
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    set_delegation(&mut deps.querier, 10, "uluna");

    let msg = HandleMsg::DeRegisterValidator {
        validator: validator.address,
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(2, res.messages.len());
}

#[test]
pub fn proper_claim_reward() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

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
}

#[test]
pub fn proper_init_burn() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(
        &mut deps,
        owner.clone(),
        reward_contract,
        token_contract.clone(),
    );

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env.clone(), token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());
    set_delegation(&mut deps.querier, 10, "uluna");

    let env = mock_env(&bob, &[]);
    let res = handle_burn(&mut deps, env, Uint128(1), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let burn = InitBurn {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract.clone(),
        amount: Uint128(5),
        msg: Some(to_binary(&burn).unwrap()),
    });
    let token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env, receive.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let msg = &res.messages[0];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(msg, &to_binary(&Burn { amount: Uint128(5) }).unwrap());
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let burn = Burn { amount: Uint128(5) };
    let env = mock_env(&bob, &[]);
    let faulty_res = token_handle(&mut deps, env, burn.clone());
    assert_eq!(faulty_res.is_err(), true);
    assert_eq!(faulty_res.unwrap_err(), StdError::unauthorized());

    let underflow_error = token_handle(&mut deps, gov_env, burn.clone());
    assert_eq!(
        underflow_error.unwrap_err(),
        StdError::Underflow {
            minuend: "0".to_string(),
            subtrahend: "5".to_string(),
            backtrace: None
        }
    );

    //mint for governance contract first
    let token_mint = Mint {
        recipient: HumanAddr::from(MOCK_CONTRACT_ADDR),
        amount: Uint128(10),
    };

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env.clone(), token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    let send = Send {
        contract: gov_env.message.sender.clone(),
        amount: Uint128(5),
        msg: Some(to_binary(&receive).unwrap()),
    };

    let env = mock_env(&bob, &[]);
    let send_res = token_handle(&mut deps, env, send).unwrap();
    assert_eq!(send_res.messages.len(), 3);

    let balance = Balance {
        address: gov_env.message.sender.clone(),
    };
    let query_balance: BalanceResponse =
        from_binary(&token_query(&deps, balance).unwrap()).unwrap();
    assert_eq!(query_balance.balance, Uint128(15));

    let burn_res = token_handle(&mut deps, gov_env.clone(), burn).unwrap();
    let message = &burn_res.messages[0];

    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr.0, "reward");
            assert_eq!(
                msg,
                &to_binary(&ClaimReward {
                    recipient: Some(HumanAddr::from(MOCK_CONTRACT_ADDR))
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    let balance = Balance {
        address: gov_env.message.sender,
    };
    let query_balance: BalanceResponse =
        from_binary(&token_query(&deps, balance).unwrap()).unwrap();
    assert_eq!(query_balance.balance, Uint128(10));
}

#[test]
pub fn proper_slashing() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(
        &mut deps,
        owner.clone(),
        reward_contract,
        token_contract.clone(),
    );

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address.clone(),
    };

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(1000u128))])]);

    let env = mock_env(&bob, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, 900, "uluna");

    let report_slashing = ReportSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = ExchangeRate {};
    let query_exchange_rate: Decimal = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.to_string(), "0.9");

    //mint again to see the final result
    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let message = &res.messages[0];
    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Mint {
                    recipient: env.message.sender,
                    amount: Uint128(1111)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    //check finish burn final
    let finish_msg = HandleMsg::FinishBurn {};

    set_delegation(&mut deps.querier, 1900, "uluna");
    let mut env = mock_env(&bob, &[]);
    let _res = handle_burn(&mut deps, env.clone(), Uint128(1000), bob.clone()).unwrap();
    set_delegation(&mut deps.querier, 1000, "uluna");

    let ex_rate = ExchangeRate {};
    let query_exchange_rate: Decimal = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.to_string(), "0.9");

    env.block.time += 90;
    let finish_res = handle(&mut deps, env, finish_msg).unwrap();
    let ex_rate = ExchangeRate {};
    let query_exchange_rate: Decimal = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.to_string(), "0.9");

    let sent_message = &finish_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(900))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }
}

#[test]
pub fn proper_finish() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());
    set_delegation(&mut deps.querier, 10, "uluna");

    let res = handle_burn(&mut deps, env, Uint128(1), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let finish_msg = HandleMsg::FinishBurn {};

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 hours from now.
    env.block.time += 30;
    let finish_res = handle(&mut deps, env.clone(), finish_msg.clone());

    assert_eq!(true, finish_res.is_err());
    assert_eq!(
        finish_res.unwrap_err(),
        StdError::generic_err("Previously requested amount is not ready yet")
    );

    env.block.time += 90;
    let finish_res = handle(&mut deps, env, finish_msg).unwrap();

    assert_eq!(finish_res.messages.len(), 1);

    let sent_message = &finish_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(1))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    let delegate = &finish_res.messages[0];
    match delegate {
        CosmosMsg::Bank(BankMsg::Send {
            from_address: _,
            to_address,
            amount,
        }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount.get(0).unwrap().amount, Uint128(1))
        }
        _ => panic!("Unexpected message: {:?}", delegate),
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

fn set_delegation(querier: &mut WasmMockQuerier, amount: u128, denom: &str) {
    querier.update_staking(
        "uluna",
        &[sample_validator(DEFAULT_VALIDATOR)],
        &[sample_delegation(DEFAULT_VALIDATOR, coin(amount, denom))],
    );
}

fn sample_delegation<U: Into<HumanAddr>>(addr: U, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = coin(0, &amount.denom);
    FullDelegation {
        validator: addr.into(),
        delegator: HumanAddr::from(MOCK_CONTRACT_ADDR),
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}
