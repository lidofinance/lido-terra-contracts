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
    coin, CosmosMsg, Decimal, HumanAddr, InitResponse, StakingMsg, Uint128, Validator,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env, MockQuerier};

use anchor_bluna::msg::{HandleMsg, InitMsg};

use anchor_bluna::contract::{handle, init};

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

fn default_init() -> InitMsg {
    InitMsg {
        name: "uluna".to_string(),
        symbol: "BLA".to_string(),
        decimals: 6,
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLA".to_string(),
        decimals: 6,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
    //TODO: query TokenInfo, query PoolInfo, query TokenState
}
#[test]
fn proper_mint() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let creator = HumanAddr::from("creator");
    let init_msg = default_init();
    let env = mock_env(&creator, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let env = mock_env(&creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address.clone(),
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna"), coin(1000, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    //test invalid validator
    let ardi = HumanAddr::from("ardi");
    let invalid_val = sample_validator("invalid");
    let mint_msg = HandleMsg::Mint {
        validator: invalid_val.address,
        amount: Uint128(5),
    };
    let env = mock_env(&ardi, &[coin(10, "uluna"), coin(1000, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).is_err();
    assert_eq!(true, res);

    //test invalid RegisterValidator sender
    let wrong_creator = HumanAddr::from("wrong_creator");
    let env = mock_env(&wrong_creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address,
    };
    let res = handle(&mut deps, env, msg).is_err();
    assert_eq!(true, res);
}

#[test]
pub fn proper_claim_reward() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let creator = HumanAddr::from("creator");
    let other_contract = HumanAddr::from("other_contract");
    let init_msg = default_init();
    let env = mock_env(&creator, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let env = mock_env(&creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let register_msg = HandleMsg::Register {};
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    let bob = HumanAddr::from("bob");

    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let reward_msg = HandleMsg::ClaimRewards { to: None };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
pub fn proper_send() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let creator = HumanAddr::from("creator");
    let other_contract = HumanAddr::from("other_contract");
    let init_msg = default_init();
    let env = mock_env(&creator, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let env = mock_env(&creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let register_msg = HandleMsg::Register {};
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    let bob = HumanAddr::from("bob");

    let mint_msg = HandleMsg::Mint {
        validator: validator.address.clone(),
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let alice = HumanAddr::from("alice");

    let alice_mint_msg = HandleMsg::Mint {
        validator: validator.address,
        amount: Uint128(5),
    };

    let alice_env = mock_env(&alice, &[coin(100, "uluna")]);
    let res = handle(&mut deps, alice_env, alice_mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let send_msg = HandleMsg::Send {
        recipient: bob.clone(),
        amount: Uint128(1),
    };
    let alice_env = mock_env(&alice, &[coin(100, "uluna")]);
    let send_res = handle(&mut deps, alice_env, send_msg).unwrap();

    assert_eq!(0, send_res.messages.len());

    //send more than balance
    let error_send_msg = HandleMsg::Send {
        recipient: bob,
        amount: Uint128(7),
    };

    let alice_env = mock_env(&alice, &[coin(100, "uluna")]);
    let error = handle(&mut deps, alice_env, error_send_msg).is_err();
    assert_eq!(true, error);
}

#[test]
pub fn proper_init_burn() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let creator = HumanAddr::from("creator");
    let other_contract = HumanAddr::from("other_contract");
    let invalid_usrer = HumanAddr::from("invalid");

    let init_msg = default_init();
    let env = mock_env(&creator, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let register_msg = HandleMsg::Register {};
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    // Test only one time we Register message can be sent.
    let error_register_msg = HandleMsg::Register {};
    let error_register_env = mock_env(&invalid_usrer, &[]);
    let error = handle(&mut deps, error_register_env, error_register_msg).is_err();
    assert_eq!(true, error);

    let env = mock_env(&creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");

    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let init_burn = HandleMsg::InitBurn { amount: Uint128(1) };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, init_burn).unwrap();
    assert_eq!(0, res.messages.len());

    //send more than the user balance
    let error_burn = HandleMsg::InitBurn { amount: Uint128(7) };

    let env = mock_env(&bob, &[coin(100, "uluna")]);
    let error = handle(&mut deps, env, error_burn).is_err();
    assert_eq!(true, error);
}

#[test]
pub fn proper_finish() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let creator = HumanAddr::from("creator");
    let other_contract = HumanAddr::from("other_contract");
    let init_msg = default_init();
    let env = mock_env(&creator, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let register_msg = HandleMsg::Register {};
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    let env = mock_env(&creator, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");

    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let init_burn = HandleMsg::InitBurn { amount: Uint128(1) };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, init_burn).unwrap();
    assert_eq!(0, res.messages.len());

    let finish_msg = HandleMsg::FinishBurn { amount: Uint128(1) };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let finish_res = handle(&mut deps, env, finish_msg).is_err();

    assert_eq!(true, finish_res);
}
