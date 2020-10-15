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
    from_binary, Coin, HandleResponse, HandleResult, HumanAddr, InitResponse, StdError,
};

use cosmwasm_std::testing ::{mock_dependencies, mock_env};
use cosmwasm_vm::testing::{
    handle, query, MockApi, MockQuerier, MockStorage,
};
use cosmwasm_vm::Instance;

use anchor_bluna::msg::InitMsg;

use anchor_bluna::contract::init;


const DEFAULT_GAS_LIMIT: u64 = 500_000;


#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLA".to_string(),
        decimals: 9
    };

    let env = mock_env("addr0000", &[]);

    // we can just call .unwrap() to assert this was a success
    let mut res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
    //TODO: query TokenInfo, query PoolInfo, query TokenState
}
