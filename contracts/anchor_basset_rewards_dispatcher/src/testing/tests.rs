//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
//! You can easily convert unit tests to integration tests as follows:
//! 1. Copy them over verbatim
//! 2. Then change
//!      let mut deps = mock_dependencies(&[]);
//!    to
//!      let mut deps = mock_instance(WASM, &[]);
//! 3. If you access raw storage, where ever you see something like:
//!      deps.storage.get(CONFIG_KEY).expect("no data stored");
//!    replace it with:
//!      deps.with_storage(|store| {
//!          let data = store.get(CONFIG_KEY).expect("no data stored");
//!          //...
//!      });
//! 4. Anywhere you see query(&deps, ...) you must replace it with query(deps.as_mut(), ...)

use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{coins, Api, Coin, Decimal, StdError, Uint128};

use crate::contract::{execute, get_swap_info, instantiate};
use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::state::CONFIG;
use crate::testing::mock_querier::{
    mock_dependencies, MOCK_BLUNA_REWARD_CONTRACT_ADDR, MOCK_HUB_CONTRACT_ADDR,
    MOCK_LIDO_FEE_ADDRESS,
};

fn default_init() -> InstantiateMsg {
    InstantiateMsg {
        hub_contract: String::from(MOCK_HUB_CONTRACT_ADDR),
        bluna_reward_contract: String::from(MOCK_BLUNA_REWARD_CONTRACT_ADDR),
        bluna_reward_denom: "uusd".to_string(),
        stluna_reward_denom: "uluna".to_string(),
        lido_fee_address: String::from(MOCK_LIDO_FEE_ADDRESS),
        lido_fee_rate: Decimal::from_ratio(Uint128::from(5u64), Uint128::from(100u64)),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let info = mock_info("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
fn test_swap_to_reward_denom() {
    struct TestCase {
        rewards_balance: Vec<Coin>,
        stluna_total_bonded: Uint128,
        bluna_total_bonded: Uint128,
        expected_total_luna_rewards_available: String,
        expected_total_ust_rewards_available: String,
        expected_offer_coin_denom: String,
        expected_offer_coin_amount: String,
        expected_ask_denom: String,
    }

    let test_cases: Vec<TestCase> = vec![
        TestCase {
            rewards_balance: vec![
                Coin::new(200, "uluna"),
                Coin::new(300, "uusd"),
                Coin::new(500, "usdr"),
            ],
            stluna_total_bonded: Uint128::from(1u128),
            bluna_total_bonded: Uint128::from(2u128),
            expected_total_luna_rewards_available: "200".to_string(),
            expected_total_ust_rewards_available: "1300".to_string(),
            expected_offer_coin_denom: "uluna".to_string(),
            expected_offer_coin_amount: "120".to_string(),
            expected_ask_denom: "uusd".to_string(),
        },
        TestCase {
            rewards_balance: vec![
                Coin::new(200, "uluna"),
                Coin::new(300, "uusd"),
                Coin::new(500, "usdr"),
            ],
            stluna_total_bonded: Uint128::from(2u128),
            bluna_total_bonded: Uint128::from(2u128),
            expected_total_luna_rewards_available: "200".to_string(),
            expected_total_ust_rewards_available: "1300".to_string(),
            expected_offer_coin_denom: "uluna".to_string(),
            expected_offer_coin_amount: "80".to_string(),
            expected_ask_denom: "uusd".to_string(),
        },
        TestCase {
            rewards_balance: vec![
                Coin::new(200, "uluna"),
                Coin::new(300, "uusd"),
                Coin::new(500, "usdr"),
            ],
            stluna_total_bonded: Uint128::from(2u128),
            bluna_total_bonded: Uint128::from(1u128),
            expected_total_luna_rewards_available: "200".to_string(),
            expected_total_ust_rewards_available: "1300".to_string(),
            expected_offer_coin_denom: "uluna".to_string(),
            expected_offer_coin_amount: "40".to_string(),
            expected_ask_denom: "uusd".to_string(),
        },
        TestCase {
            rewards_balance: vec![
                Coin::new(0, "uluna"),
                Coin::new(300, "uusd"),
                Coin::new(500, "usdr"),
            ],
            stluna_total_bonded: Uint128::from(2u128),
            bluna_total_bonded: Uint128::from(2u128),
            expected_total_luna_rewards_available: "0".to_string(),
            expected_total_ust_rewards_available: "1300".to_string(),
            expected_offer_coin_denom: "uusd".to_string(),
            expected_offer_coin_amount: "640".to_string(),
            expected_ask_denom: "uluna".to_string(),
        },
    ];

    for test_case in test_cases {
        let mut deps = mock_dependencies(&test_case.rewards_balance);

        let msg = default_init();
        let info = mock_info("creator", &[]);

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        let info = mock_info(String::from(MOCK_HUB_CONTRACT_ADDR).as_str(), &[]);
        let msg = ExecuteMsg::SwapToRewardDenom {
            stluna_total_bonded: test_case.stluna_total_bonded,
            bluna_total_bonded: test_case.bluna_total_bonded,
        };

        let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();

        for attr in res.attributes {
            if attr.key == *"total_luna_rewards_available" {
                assert_eq!(attr.value, test_case.expected_total_luna_rewards_available)
            }
            if attr.key == *"total_ust_rewards_available" {
                assert_eq!(attr.value, test_case.expected_total_ust_rewards_available)
            }
            if attr.key == *"offer_coin_denom" {
                assert_eq!(attr.value, test_case.expected_offer_coin_denom)
            }
            if attr.key == *"offer_coin_amount" {
                assert_eq!(attr.value, test_case.expected_offer_coin_amount)
            }
            if attr.key == *"ask_denom" {
                assert_eq!(attr.value, test_case.expected_ask_denom)
            }
        }
    }
}

#[test]
fn test_dispatch_rewards() {
    let mut deps = mock_dependencies(&[
        Coin::new(200, "uluna"),
        Coin::new(300, "uusd"),
        Coin::new(20, "usdr"),
    ]);

    let msg = default_init();
    let info = mock_info("creator", &[]);

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let info = mock_info(String::from(MOCK_HUB_CONTRACT_ADDR).as_str(), &[]);
    let msg = ExecuteMsg::DispatchRewards {};

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(4, res.messages.len());

    for attr in res.attributes {
        if attr.key == "stluna_rewards" {
            assert_eq!("188uluna", attr.value)
        }
        if attr.key == "bluna_rewards" {
            assert_eq!("282uusd", attr.value)
        }
        if attr.key == "lido_stluna_fee" {
            assert_eq!("9uluna", attr.value)
        }
        if attr.key == "lido_bluna_fee" {
            assert_eq!("14uusd", attr.value)
        }
    }
}

#[test]
fn test_get_swap_info() {
    let mut deps = mock_dependencies(&[]);

    let msg = default_init();
    let info = mock_info("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = CONFIG.load(&deps.storage).unwrap();

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_luna_rewards_available = Uint128::from(20u64);
    let total_ust_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(1u64), Uint128::from(1u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(1u64), Uint128::from(1u64));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_luna_rewards_available,
        total_ust_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.bluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128::zero());

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_luna_rewards_available = Uint128::from(20u64);
    let total_ust_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(15u64), Uint128::from(10u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(10u64), Uint128::from(15u64));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_luna_rewards_available,
        total_ust_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.bluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128::from(3u64));

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_luna_rewards_available = Uint128::from(20u64);
    let total_ust_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(75u64), Uint128::from(100u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(100u64), Uint128::from(75u64));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_luna_rewards_available,
        total_ust_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.stluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128::from(3u64));
}

#[test]
fn test_update_config() {
    let mut deps = mock_dependencies(&[]);

    let owner = String::from("creator");
    let msg = default_init();
    let info = mock_info(&owner, &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    //check call from invalid owner
    let invalid_owner = String::from("invalid_owner");
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: Some(String::from("some_addr")),
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&invalid_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // change owner
    let new_owner = String::from("new_owner");
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: Some(new_owner.clone()),
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    let new_owner_raw = deps.api.addr_canonicalize(&new_owner).unwrap();
    assert_eq!(new_owner_raw, config.owner);

    // change hub_contract
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: Some(String::from("some_address")),
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .addr_canonicalize(&String::from("some_address"))
            .unwrap(),
        config.hub_contract
    );

    // change bluna_reward_contract
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: Some(String::from("some_address")),
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .addr_canonicalize(&String::from("some_address"))
            .unwrap(),
        config.bluna_reward_contract
    );

    // change stluna_reward_denom
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: Some(String::from("new_denom")),
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.stluna_reward_denom);

    // change bluna_reward_denom
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: Some(String::from("new_denom")),
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.bluna_reward_denom);

    // change lido_fee_address
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: Some(String::from("some_address")),
        lido_fee_rate: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .addr_canonicalize(&String::from("some_address"))
            .unwrap(),
        config.lido_fee_address
    );

    // change lido_fee_rate
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: Some(Decimal::one()),
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(Decimal::one(), config.lido_fee_rate);
}
