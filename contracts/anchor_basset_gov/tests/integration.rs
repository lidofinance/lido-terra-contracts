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
    coin, from_binary, Api, BankMsg, CosmosMsg, Decimal, Extern, HumanAddr, InitResponse, Querier,
    StakingMsg, Storage, Uint128, Validator,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env, MockQuerier};

use anchor_bluna::msg::{HandleMsg, InitMsg, QueryMsg, TokenInfoResponse};

use anchor_bluna::contract::{handle, init, query};

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
        code_id: 0,
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLA".to_string(),
        decimals: 6,
        code_id: 0,
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());

    //check token_info
    let token_query = QueryMsg::TokenInfo {};
    let query_result = query(&deps, token_query).unwrap();
    let value: TokenInfoResponse = from_binary(&query_result).unwrap();
    assert_eq!("bluna".to_string(), value.name);
    assert_eq!("BLA".to_string(), value.symbol);
    assert_eq!(Uint128::zero(), value.total_supply);
    assert_eq!(6, value.decimals);
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
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    //check the balance of the bob
    let balance_msg = QueryMsg::Balance { address: bob };
    let query_result = query(&deps, balance_msg).unwrap();
    let value: Uint128 = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(5), value);

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
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let reward_msg = HandleMsg::ClaimRewards { to: None };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let reward_query = QueryMsg::AccruedRewards { address: bob };
    let q = query(&deps, reward_query).unwrap();
    let query_result: Uint128 = from_binary(&q).unwrap();
    assert_eq!(Uint128(0), query_result);
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

    //query whitelisted validators
    let valid_query = QueryMsg::WhiteListedValidators {};
    let query_result = query(&deps, valid_query).unwrap();
    let validators: Vec<HumanAddr> = from_binary(&query_result).unwrap();
    let white_valid = validators.get(0).unwrap();
    assert_eq!(&validator.address, white_valid);

    let register_msg = HandleMsg::Register {};
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    let bob = HumanAddr::from("bob");

    let mint_msg = HandleMsg::Mint {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let alice = HumanAddr::from("alice");

    let alice_mint_msg = HandleMsg::Mint {
        validator: validator.address,
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

    assert_eq!(1, send_res.messages.len());

    //check the balance of the bob
    let balance_msg = QueryMsg::Balance {
        address: bob.clone(),
    };
    let query_result = query(&deps, balance_msg).unwrap();
    let value: Uint128 = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(6), value);

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
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let init_burn = HandleMsg::InitBurn { amount: Uint128(1) };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, init_burn).unwrap();
    assert_eq!(1, res.messages.len());

    let balance = QueryMsg::Balance {
        address: bob.clone(),
    };
    let query_result = query(&deps, balance).unwrap();
    let value: Uint128 = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(4), value);

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
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let init_burn = HandleMsg::InitBurn { amount: Uint128(1) };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, init_burn).unwrap();
    assert_eq!(1, res.messages.len());

    send_init_burn(&mut deps, "ardi", validator);

    let finish_msg = HandleMsg::FinishBurn { amount: Uint128(1) };

    let mut env = mock_env(&bob, &[coin(10, "uluna")]);
    //set the block time 6 hours from now.
    env.block.time += 22600;
    let finish_res = handle(&mut deps, env.clone(), finish_msg.clone()).is_err();

    assert_eq!(true, finish_res);

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

fn send_init_burn<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    address_string: &str,
    validator: Validator,
) {
    let bob = HumanAddr::from(address_string);

    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let init_burn = HandleMsg::InitBurn { amount: Uint128(1) };

    let mut env = mock_env(&bob, &[coin(10, "uluna")]);
    env.block.time += 22600;
    let res = handle(&mut deps, env, init_burn).unwrap();
    assert_eq!(2, res.messages.len());
}
