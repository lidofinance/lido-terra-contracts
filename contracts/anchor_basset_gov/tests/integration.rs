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
    coin, from_binary, Coin, CosmosMsg, Decimal, HandleResponse, HandleResult, HumanAddr,
    InitResponse, StakingMsg, StdError, Uint128, Validator,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env, MockQuerier};

use anchor_bluna::msg::{HandleMsg, InitMsg};

use anchor_bluna::contract::{handle, init};

const DEFAULT_GAS_LIMIT: u64 = 500_000;
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
        decimals: 9,
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLA".to_string(),
        decimals: 9,
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

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
        amount: Uint128(5),
    };

    let env = mock_env(&bob, &[coin(10, "uluna"), coin(1000, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(1000, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }
}
