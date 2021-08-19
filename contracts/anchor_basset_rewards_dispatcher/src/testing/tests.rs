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
use crate::state::read_config;
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
    let mut deps = mock_dependencies(&[
        Coin::new(20, "uluna"),
        Coin::new(20, "uusd"),
        Coin::new(20, "usdr"),
    ]);

    let msg = default_init();
    let info = mock_info("creator", &[]);

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let info = mock_info(String::from(MOCK_HUB_CONTRACT_ADDR).as_str(), &[]);
    let msg = ExecuteMsg::SwapToRewardDenom {
        stluna_total_mint_amount: Uint128::from(2u64),
        bluna_total_mint_amount: Uint128::from(2u64),
    };

    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(2, res.messages.len());
}

#[test]
fn test_dispatch_rewards() {
    let mut deps = mock_dependencies(&[
        Coin::new(20, "uluna"),
        Coin::new(30, "uusd"),
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
        if attr.key == "stluna_rewards_denom" {
            assert_eq!("uluna", attr.value)
        }
        if attr.key == "stluna_rewards_amount" {
            assert_eq!("19", attr.value)
        }
        if attr.key == "bluna_rewards_denom" {
            assert_eq!("uusd", attr.value)
        }
        if attr.key == "bluna_rewards_amount" {
            assert_eq!("28", attr.value)
        }
        if attr.key == "lido_stluna_fee" {
            assert_eq!("1", attr.value)
        }
        if attr.key == "lido_bluna_fee" {
            assert_eq!("2", attr.value)
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

    let config = read_config(&deps.storage).unwrap();

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_stluna_rewards_available = Uint128::from(20u64);
    let total_bluna_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(1u64), Uint128::from(1u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(1u64), Uint128::from(1u64));
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
    assert_eq!(offer_coin.amount, Uint128::zero());

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_stluna_rewards_available = Uint128::from(20u64);
    let total_bluna_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(15u64), Uint128::from(10u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(10u64), Uint128::from(15u64));
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
    assert_eq!(offer_coin.amount, Uint128::from(3u64));

    let stluna_total_bond_amount = Uint128::from(2u64);
    let bluna_total_bond_amount = Uint128::from(2u64);
    let total_stluna_rewards_available = Uint128::from(20u64);
    let total_bluna_rewards_available = Uint128::from(20u64);
    let bluna_2_stluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(75u64), Uint128::from(100u64));
    let stluna_2_bluna_rewards_xchg_rate =
        Decimal::from_ratio(Uint128::from(100u64), Uint128::from(75u64));
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
    };
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    let new_owner_raw = deps.api.addr_canonicalize(&new_owner).unwrap();
    assert_eq!(new_owner_raw, config.owner);

    // change hub_contract
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: Some(String::from("some_address")),
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: None,
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
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
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
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
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.stluna_reward_denom);

    // change bluna_reward_denom
    let update_config_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        hub_contract: None,
        bluna_reward_contract: None,
        stluna_reward_denom: None,
        bluna_reward_denom: Some(String::from("new_denom")),
    };
    let info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config_msg);
    assert!(res.is_ok());

    let config = read_config(&deps.storage).unwrap();
    assert_eq!(String::from("new_denom"), config.bluna_reward_denom);
}
