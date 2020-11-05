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
    coin, from_binary, Api, BankMsg, CanonicalAddr, CosmosMsg, Decimal, Extern, HumanAddr,
    InitResponse, Querier, StakingMsg, Storage, Uint128, Validator,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env, MockQuerier};

use anchor_bluna::msg::InitMsg;
use gov_courier::HandleMsg;

use anchor_bluna::contract::{handle, handle_burn, init};

use anchor_basset_reward::contracts::init as reward_init;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_token::contract::{
    handle as token_handle, init as token_init, query as token_query,
};
use anchor_basset_token::msg::HandleMsg::Mint;
use anchor_basset_token::msg::QueryMsg::{Balance, TokenInfo};
use anchor_basset_token::msg::TokenInitMsg;
use cw20::{BalanceResponse, MinterResponse, TokenInfoResponse};
use gov_courier::Registration::{Reward, Token};

const DEFAULT_VALIDATOR: &str = "default-validator";

fn sample_validator<U: Into<HumanAddr>>(addr: U) -> Validator {
    Validator {
        address: addr.into(),
        commission: Decimal::percent(3),
        max_commission: Decimal::percent(10),
        max_change_rate: Decimal::percent(1),
    }
}

fn set_validator(querier: &mut MockQuerier) {
    querier.update_staking("uluna", &[sample_validator(DEFAULT_VALIDATOR)], &[]);
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

    let owner_raw = deps.api.canonical_address(&owner).unwrap();

    let env = mock_env(owner.clone(), &[]);

    init(&mut deps, env.clone(), msg).unwrap();

    let reward_in = default_reward(owner_raw.clone());
    reward_init(&mut deps, env.clone(), reward_in).unwrap();

    let token_int = default_token(owner_raw, owner);
    token_init(&mut deps, env, token_int).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Reward };
    let register_env = mock_env(reward_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Token };
    let register_env = mock_env(token_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();
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

    let owner = HumanAddr::from("addr0000");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();

    let env = mock_env(owner.clone(), &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env.clone(), msg).unwrap();
    assert_eq!(2, res.messages.len());

    let reward_in = default_reward(owner_raw.clone());
    reward_init(&mut deps, env.clone(), reward_in).unwrap();

    let token_int = default_token(owner_raw, owner);
    token_init(&mut deps, env, token_int).unwrap();

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
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(3, res.messages.len());

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
    let token_res = token_handle(&mut deps, owner_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    //check the balance of the bob
    let balance_msg = Balance { address: bob };
    let query_result = token_query(&deps, balance_msg).unwrap();
    let value: BalanceResponse = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(10), value.balance);
}

#[test]
pub fn proper_claim_reward() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner.clone(), &[]);
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

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(3, res.messages.len());

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
    let token_res = token_handle(&mut deps, owner_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());
}

#[test]
pub fn proper_init_burn() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner.clone(), &[]);

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

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(3, res.messages.len());

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
    let token_res = token_handle(&mut deps, owner_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let env = mock_env(&bob, &[]);
    let res = handle_burn(&mut deps, env, Uint128(1), bob).unwrap();
    assert_eq!(1, res.messages.len());
}

#[test]
pub fn proper_finish() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner.clone(), &[]);
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

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
    assert_eq!(3, res.messages.len());

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
    let token_res = token_handle(&mut deps, owner_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let res = handle_burn(&mut deps, env, Uint128(1), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let finish_msg = HandleMsg::FinishBurn { amount: Uint128(1) };

    let mut env = mock_env(&bob, &[coin(10, "uluna")]);
    //set the block time 6 hours from now.
    env.block.time += 22600;
    let finish_res = handle(&mut deps, env.clone(), finish_msg.clone()).is_err();

    assert_eq!(false, finish_res);

    env.block.time = 1573911419;
    let finish_res = handle(&mut deps, env, finish_msg).unwrap();

    assert_eq!(finish_res.messages.len(), 1);

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
