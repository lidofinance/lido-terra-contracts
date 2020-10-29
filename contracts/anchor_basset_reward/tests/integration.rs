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

use cosmwasm_std::{coin, Api, CanonicalAddr, HumanAddr};

use cosmwasm_std::testing::{mock_dependencies, mock_env};

use anchor_basset_reward::init::RewardInitMsg;

use anchor_basset_reward::contracts::{handle, init};

use anchor_basset_reward::msg::HandleMsg;

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

    let env = mock_env("addr0000", &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());
}
#[test]
pub fn proper_send_reward() {
    let mut deps = mock_dependencies(20, &[]);
    let owner = HumanAddr::from("owner");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let init_msg = default_init(owner_raw);

    let env = mock_env("addr0000", &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env(&owner, &[coin(10, "uluna"), coin(1000, "uluna")]);

    let _alice = HumanAddr::from("alice");
    let msg = HandleMsg::SendReward {};

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
}

#[test]
pub fn proper_swap() {
    let mut deps = mock_dependencies(20, &[]);
    let owner = HumanAddr::from("owner");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let init_msg = default_init(owner_raw);

    let env = mock_env("addr0000", &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let env = mock_env(&owner, &[coin(10, "uluna"), coin(1000, "uluna")]);

    let msg = HandleMsg::Swap {};

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());
}
