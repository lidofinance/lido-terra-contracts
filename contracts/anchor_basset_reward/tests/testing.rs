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

use cosmwasm_std::{Api, CanonicalAddr, HumanAddr, StdError};

use cosmwasm_std::testing::{mock_dependencies, mock_env};

use anchor_basset_reward::init::RewardInitMsg;

use anchor_basset_reward::contracts::{handle, init};

use anchor_basset_reward::msg::HandleMsg;
use anchor_basset_reward::msg::HandleMsg::UpdateParams;

fn default_init(owner: CanonicalAddr) -> RewardInitMsg {
    RewardInitMsg {
        owner,
        init_hook: None,
    }
}

#[test]
pub fn proper_init() {
    let mut deps = mock_dependencies(20, &[]);
    let owner = HumanAddr::from("owner");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let init_msg = default_init(owner_raw);

    let env = mock_env("owner", &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
pub fn proper_swap() {
    let mut deps = mock_dependencies(20, &[]);
    let owner = HumanAddr::from("owner");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let init_msg = default_init(owner_raw);

    let env = mock_env(owner, &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let update_params = UpdateParams {
        swap_denom: "uusd".to_string(),
    };

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    let res = handle(&mut deps, env, update_params).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::SwapToRewardDenom {};

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
pub fn proper_update_params() {
    let mut deps = mock_dependencies(20, &[]);
    let owner = HumanAddr::from("owner");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let init_msg = default_init(owner_raw);
    let env = mock_env(owner, &[]);
    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    //send an invalid user
    let update_params = UpdateParams {
        swap_denom: "uusd".to_string(),
    };

    let fake = HumanAddr::from("invalid");
    let env = mock_env(&fake, &[]);

    let res = handle(&mut deps, env, update_params);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let update_params = UpdateParams {
        swap_denom: "uusd".to_string(),
    };

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    let res = handle(&mut deps, env, update_params).unwrap();
    assert_eq!(0, res.messages.len());
}
