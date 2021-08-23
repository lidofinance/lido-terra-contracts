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

use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{coins, Api, Coin, Decimal, HumanAddr, StdError, Uint128};

use crate::contract::{get_swap_info, handle, init};
use crate::msg::{HandleMsg, InitMsg};
use crate::state::read_config;
use crate::testing::mock_querier::{
    mock_dependencies, MOCK_BLUNA_REWARD_CONTRACT_ADDR, MOCK_HUB_CONTRACT_ADDR,
    MOCK_LIDO_FEE_ADDRESS,
};

fn default_init() -> InitMsg {
    InitMsg {
        hub_contract: HumanAddr::from(MOCK_HUB_CONTRACT_ADDR),
        bluna_reward_contract: HumanAddr::from(MOCK_BLUNA_REWARD_CONTRACT_ADDR),
        bluna_reward_denom: "uusd".to_string(),
        stluna_reward_denom: "uluna".to_string(),
        lido_fee_address: HumanAddr::from(MOCK_LIDO_FEE_ADDRESS),
        lido_fee_rate: Decimal::from_ratio(Uint128(5), Uint128(100)),
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
fn test_swap_to_reward_denom() {
    let mut deps = mock_dependencies(
        20,
        &[
            Coin::new(20, "uluna"),
            Coin::new(20, "uusd"),
            Coin::new(20, "usdr"),
        ],
    );

    let msg = default_init();
    let env = mock_env("creator", &[]);

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env(HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), &[]);
    let msg = HandleMsg::SwapToRewardDenom {
        stluna_total_mint_amount: Uint128(2),
        bluna_total_mint_amount: Uint128(2),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(2, res.messages.len());
}

#[test]
fn test_dispatch_rewards() {
    let mut deps = mock_dependencies(
        20,
        &[
            Coin::new(200, "uluna"),
            Coin::new(300, "uusd"),
            Coin::new(20, "usdr"),
        ],
    );

    let msg = default_init();
    let env = mock_env("creator", &[]);

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env(HumanAddr::from(MOCK_HUB_CONTRACT_ADDR), &[]);
    let msg = HandleMsg::DispatchRewards {};

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(4, res.messages.len());

    for log in res.log {
        if log.key == "stluna_rewards_denom" {
            assert_eq!("uluna", log.value)
        }
        if log.key == "stluna_rewards_amount" {
            assert_eq!("190", log.value)
        }
        if log.key == "bluna_rewards_denom" {
            assert_eq!("uusd", log.value)
        }
        if log.key == "bluna_rewards_amount" {
            assert_eq!("282", log.value)
        }
        if log.key == "lido_stluna_fee_amount" {
            assert_eq!("9", log.value)
        }
        if log.key == "lido_bluna_fee_amount" {
            assert_eq!("14", log.value)
        }
    }
}

#[test]
fn test_get_swap_info() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = default_init();
    let env = mock_env("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let config = read_config(&deps.storage).unwrap();

    let stluna_total_bond_amount = Uint128(2);
    let bluna_total_bond_amount = Uint128(2);
    let total_stluna_rewards_available = Uint128(20);
    let total_bluna_rewards_available = Uint128(20);
    let bluna_2_stluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(1), Uint128(1));
    let stluna_2_bluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(1), Uint128(1));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_stluna_rewards_available,
        total_bluna_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.bluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128(0));

    let stluna_total_bond_amount = Uint128(2);
    let bluna_total_bond_amount = Uint128(2);
    let total_stluna_rewards_available = Uint128(20);
    let total_bluna_rewards_available = Uint128(20);
    let bluna_2_stluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(15), Uint128(10));
    let stluna_2_bluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(10), Uint128(15));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_stluna_rewards_available,
        total_bluna_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.bluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128(3));

    let stluna_total_bond_amount = Uint128(2);
    let bluna_total_bond_amount = Uint128(2);
    let total_stluna_rewards_available = Uint128(20);
    let total_bluna_rewards_available = Uint128(20);
    let bluna_2_stluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(75), Uint128(100));
    let stluna_2_bluna_rewards_xchg_rate = Decimal::from_ratio(Uint128(100), Uint128(75));
    let (offer_coin, _) = get_swap_info(
        config.clone(),
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_stluna_rewards_available,
        total_bluna_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    )
    .unwrap();
    assert_eq!(offer_coin.denom, config.stluna_reward_denom);
    assert_eq!(offer_coin.amount, Uint128(3));
}

#[test]
fn test_update_config() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("creator");
    let msg = default_init();
    let env = mock_env(&owner, &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    //check call from invalid owner
    let invalid_owner = HumanAddr::from("invalid_owner");
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: Some(HumanAddr::from("some_addr")),
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&invalid_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // change owner
    let new_owner = HumanAddr::from("new_owner");
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: Some(new_owner.clone()),
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    let new_owner_raw = deps.api.canonical_address(&new_owner).unwrap();
    assert_eq!(new_owner_raw, config.owner);

    // change hub_contract
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: Some(HumanAddr::from("some_address")),
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .canonical_address(&HumanAddr::from("some_address"))
            .unwrap(),
        config.hub_contract
    );

    // change bluna_reward_contract
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: Some(HumanAddr::from("some_address")),
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .canonical_address(&HumanAddr::from("some_address"))
            .unwrap(),
        config.bluna_reward_contract
    );

    // change stluna_reward_denom
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: Some(String::from("new_denom")),
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.stluna_reward_denom);

    // change bluna_reward_denom
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: Some(String::from("new_denom")),
        lido_fee_address: None,
        lido_fee_rate: None,
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.bluna_reward_denom);

    // change lido_fee_address
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: Some(HumanAddr::from("some_address")),
        lido_fee_rate: None,
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(
        deps.api
            .canonical_address(&HumanAddr::from("some_address"))
            .unwrap(),
        config.lido_fee_address
    );

    // change lido_fee_rate
    let update_config_msg = HandleMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
        lido_fee_address: None,
        lido_fee_rate: Some(Decimal::one()),
    };
    let env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, env, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(Decimal::one(), config.lido_fee_rate);
}
