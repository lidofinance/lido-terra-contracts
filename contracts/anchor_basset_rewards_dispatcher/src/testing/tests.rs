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
use cosmwasm_std::{coins, Coin, Decimal, HumanAddr, Uint128};

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
        stluna_total_bond_amount: Uint128(2),
        bluna_total_bond_amount: Uint128(2),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(2, res.messages.len());
}

#[test]
fn test_dispatch_rewards() {
    let mut deps = mock_dependencies(
        20,
        &[
            Coin::new(20, "uluna"),
            Coin::new(30, "uusd"),
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
            assert_eq!("19", log.value)
        }
        if log.key == "bluna_rewards_denom" {
            assert_eq!("uusd", log.value)
        }
        if log.key == "bluna_rewards_amount" {
            assert_eq!("28", log.value)
        }
        if log.key == "lido_stluna_fee" {
            assert_eq!("1", log.value)
        }
        if log.key == "lido_bluna_fee" {
            assert_eq!("2", log.value)
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
