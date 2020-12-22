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

use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{from_binary, BankMsg, Coin, CosmosMsg, Decimal, HumanAddr, StdError, Uint128};
use terra_cosmwasm::create_swap_msg;

use crate::contracts::{handle, init, query};
use crate::msg::{
    ConfigResponse, HandleMsg, HolderResponse, HoldersResponse, InitMsg, QueryMsg, StateResponse,
};
use crate::state::{store_state, State};
use crate::testing::mock_querier::{
    mock_dependencies, MOCK_HUB_CONTRACT_ADDR, MOCK_TOKEN_CONTRACT_ADDR,
};

const DEFAULT_REWARD_DENOM: &str = "uusd";

fn default_init() -> InitMsg {
    InitMsg {
        hub_contract: HumanAddr::from(MOCK_HUB_CONTRACT_ADDR),
        reward_denom: DEFAULT_REWARD_DENOM.to_string(),
    }
}

#[test]
fn proper_init() {
    let mut deps = mock_dependencies(20, &[]);
    let init_msg = default_init();

    let env = mock_env("addr0000", &[]);

    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config_response: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config_response,
        ConfigResponse {
            hub_contract: HumanAddr::from(MOCK_HUB_CONTRACT_ADDR),
            reward_denom: DEFAULT_REWARD_DENOM.to_string(),
        }
    );

    let res = query(&deps, QueryMsg::State {}).unwrap();
    let state_response: StateResponse = from_binary(&res).unwrap();
    assert_eq!(
        state_response,
        StateResponse {
            global_index: Decimal::zero(),
            total_balance: Uint128(0u128),
        }
    );
}

#[test]
fn update_params() {
    let mut deps = mock_dependencies(20, &[]);
    let init_msg = default_init();

    let env = mock_env("addr0000", &[]);

    init(&mut deps, env.clone(), init_msg).unwrap();

    let msg = HandleMsg::UpdateParams {
        reward_denom: Some("ukrw".to_string()),
    };

    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    handle(&mut deps, env, msg).unwrap();

    let res = query(&deps, QueryMsg::Config {}).unwrap();
    let config_response: ConfigResponse = from_binary(&res).unwrap();
    assert_eq!(
        config_response,
        ConfigResponse {
            hub_contract: HumanAddr::from(MOCK_HUB_CONTRACT_ADDR),
            reward_denom: "ukrw".to_string(),
        }
    );
}

#[test]
pub fn swap_to_reward_denom() {
    let mut deps = mock_dependencies(
        20,
        &[
            Coin {
                denom: "uusd".to_string(),
                amount: Uint128(100u128),
            },
            Coin {
                denom: "ukrw".to_string(),
                amount: Uint128(1000u128),
            },
            Coin {
                denom: "usdr".to_string(),
                amount: Uint128(50u128),
            },
        ],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let env = mock_env("addr0000", &[]);
    let msg = HandleMsg::SwapToRewardDenom {};

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![
            create_swap_msg(
                HumanAddr::from(MOCK_CONTRACT_ADDR),
                Coin {
                    denom: "ukrw".to_string(),
                    amount: Uint128(1000u128),
                },
                DEFAULT_REWARD_DENOM.to_string()
            ),
            create_swap_msg(
                HumanAddr::from(MOCK_CONTRACT_ADDR),
                Coin {
                    denom: "usdr".to_string(),
                    amount: Uint128(50u128)
                },
                DEFAULT_REWARD_DENOM.to_string()
            ),
        ]
    );
}

#[test]
fn update_global_index() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(100u128),
        }],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let msg = HandleMsg::UpdateGlobalIndex {
        prev_balance: Uint128::zero(),
    };

    // Failed unauthorized try
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    }

    // Failed zero staking balance
    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let res = handle(&mut deps, env.clone(), msg.clone());
    match res {
        Err(StdError::GenericErr { msg, .. }) => assert_eq!(msg, "zero staking balance"),
        _ => panic!("DO NOT ENTER HERE"),
    }

    store_state(
        &mut deps.storage,
        &State {
            global_index: Decimal::zero(),
            total_balance: Uint128::from(100u128),
        },
    )
    .unwrap();

    // claimed_rewards = 100, total_balance = 100
    // global_index == 1
    handle(&mut deps, env, msg).unwrap();

    let res = query(&deps, QueryMsg::State {}).unwrap();
    let state_response: StateResponse = from_binary(&res).unwrap();
    assert_eq!(
        state_response,
        StateResponse {
            global_index: Decimal::one(),
            total_balance: Uint128::from(100u128),
        }
    );
}

#[test]
fn increase_balance() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(100u128),
        }],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };

    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    };

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    handle(&mut deps, env, msg).unwrap();

    let res = query(
        &deps,
        QueryMsg::Holder {
            address: HumanAddr::from("addr0000"),
        },
    )
    .unwrap();
    let holder_response: HolderResponse = from_binary(&res).unwrap();
    assert_eq!(
        holder_response,
        HolderResponse {
            address: HumanAddr::from("addr0000"),
            balance: Uint128::from(100u128),
            index: Decimal::zero(),
            pending_rewards: Uint128::zero(),
        }
    );

    // claimed_rewards = 100, total_balance = 100
    // global_index == 1
    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::UpdateGlobalIndex {
        prev_balance: Uint128::zero(),
    };
    handle(&mut deps, env, msg).unwrap();

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };
    handle(&mut deps, env, msg).unwrap();

    let res = query(
        &deps,
        QueryMsg::Holder {
            address: HumanAddr::from("addr0000"),
        },
    )
    .unwrap();
    let holder_response: HolderResponse = from_binary(&res).unwrap();
    assert_eq!(
        holder_response,
        HolderResponse {
            address: HumanAddr::from("addr0000"),
            balance: Uint128::from(200u128),
            index: Decimal::one(),
            pending_rewards: Uint128::from(100u128),
        }
    );
}

#[test]
fn decrease_balance() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(100u128),
        }],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let msg = HandleMsg::DecreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };

    // Failed unautorized
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg.clone());
    match res {
        Err(StdError::Unauthorized { .. }) => {}
        _ => panic!("DO NOT ENTER HERE"),
    };

    // Failed underflow
    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    let res = handle(&mut deps, env, msg);
    match res {
        Err(StdError::GenericErr { msg, .. }) => {
            assert_eq!(msg, "cannot decrease more than the user balance")
        }
        _ => panic!("DO NOT ENTER HERE"),
    };

    // Increase balance first
    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    handle(&mut deps, env, msg).unwrap();

    // claimed_rewards = 100, total_balance = 100
    // global_index == 1
    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::UpdateGlobalIndex {
        prev_balance: Uint128::zero(),
    };
    handle(&mut deps, env, msg).unwrap();

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::DecreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };
    handle(&mut deps, env, msg).unwrap();

    let res = query(
        &deps,
        QueryMsg::Holder {
            address: HumanAddr::from("addr0000"),
        },
    )
    .unwrap();
    let holder_response: HolderResponse = from_binary(&res).unwrap();
    assert_eq!(
        holder_response,
        HolderResponse {
            address: HumanAddr::from("addr0000"),
            balance: Uint128::zero(),
            index: Decimal::one(),
            pending_rewards: Uint128::from(100u128),
        }
    );
}

#[test]
fn claim_rewards() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(100u128),
        }],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    handle(&mut deps, env, msg).unwrap();

    let res = query(
        &deps,
        QueryMsg::Holder {
            address: HumanAddr::from("addr0000"),
        },
    )
    .unwrap();
    let holder_response: HolderResponse = from_binary(&res).unwrap();
    assert_eq!(
        holder_response,
        HolderResponse {
            address: HumanAddr::from("addr0000"),
            balance: Uint128::from(100u128),
            index: Decimal::zero(),
            pending_rewards: Uint128::zero(),
        }
    );

    // claimed_rewards = 100, total_balance = 100
    // global_index == 1
    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::UpdateGlobalIndex {
        prev_balance: Uint128::zero(),
    };
    handle(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::ClaimRewards { recipient: None };
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Bank(BankMsg::Send {
            from_address: HumanAddr::from(MOCK_CONTRACT_ADDR),
            to_address: HumanAddr::from("addr0000"),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(99u128), // 1% tax
            },]
        })]
    );

    // Set recipient
    // claimed_rewards = 100, total_balance = 100
    // global_index == 1
    let env = mock_env(MOCK_HUB_CONTRACT_ADDR, &[]);
    let msg = HandleMsg::UpdateGlobalIndex {
        prev_balance: Uint128::zero(),
    };
    handle(&mut deps, env, msg).unwrap();

    let msg = HandleMsg::ClaimRewards {
        recipient: Some(HumanAddr::from("addr0001")),
    };
    let env = mock_env("addr0000", &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Bank(BankMsg::Send {
            from_address: HumanAddr::from(MOCK_CONTRACT_ADDR),
            to_address: HumanAddr::from("addr0001"),
            amount: vec![Coin {
                denom: "uusd".to_string(),
                amount: Uint128::from(99u128), // 1% tax
            },]
        })]
    );
}

#[test]
fn query_holders() {
    let mut deps = mock_dependencies(
        20,
        &[Coin {
            denom: "uusd".to_string(),
            amount: Uint128(100u128),
        }],
    );

    let init_msg = default_init();
    let env = mock_env("addr0000", &[]);

    init(&mut deps, env, init_msg).unwrap();

    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0000"),
        amount: Uint128::from(100u128),
    };

    let env = mock_env(MOCK_TOKEN_CONTRACT_ADDR, &[]);
    handle(&mut deps, env.clone(), msg).unwrap();

    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0001"),
        amount: Uint128::from(200u128),
    };

    handle(&mut deps, env.clone(), msg).unwrap();
    let msg = HandleMsg::IncreaseBalance {
        address: HumanAddr::from("addr0002"),
        amount: Uint128::from(300u128),
    };

    handle(&mut deps, env, msg).unwrap();

    let res = query(
        &deps,
        QueryMsg::Holders {
            start_after: None,
            limit: None,
        },
    )
    .unwrap();
    let holders_response: HoldersResponse = from_binary(&res).unwrap();
    assert_eq!(
        holders_response,
        HoldersResponse {
            holders: vec![
                HolderResponse {
                    address: HumanAddr::from("addr0000"),
                    balance: Uint128::from(100u128),
                    index: Decimal::zero(),
                    pending_rewards: Uint128::zero(),
                },
                HolderResponse {
                    address: HumanAddr::from("addr0001"),
                    balance: Uint128::from(200u128),
                    index: Decimal::zero(),
                    pending_rewards: Uint128::zero(),
                },
                HolderResponse {
                    address: HumanAddr::from("addr0002"),
                    balance: Uint128::from(300u128),
                    index: Decimal::zero(),
                    pending_rewards: Uint128::zero(),
                }
            ],
        }
    );

    // Set limit
    let res = query(
        &deps,
        QueryMsg::Holders {
            start_after: None,
            limit: Some(1),
        },
    )
    .unwrap();
    let holders_response: HoldersResponse = from_binary(&res).unwrap();
    assert_eq!(
        holders_response,
        HoldersResponse {
            holders: vec![HolderResponse {
                address: HumanAddr::from("addr0000"),
                balance: Uint128::from(100u128),
                index: Decimal::zero(),
                pending_rewards: Uint128::zero(),
            }],
        }
    );

    // Set start_after
    let res = query(
        &deps,
        QueryMsg::Holders {
            start_after: Some(HumanAddr::from("addr0000")),
            limit: None,
        },
    )
    .unwrap();
    let holders_response: HoldersResponse = from_binary(&res).unwrap();
    assert_eq!(
        holders_response,
        HoldersResponse {
            holders: vec![
                HolderResponse {
                    address: HumanAddr::from("addr0001"),
                    balance: Uint128::from(200u128),
                    index: Decimal::zero(),
                    pending_rewards: Uint128::zero(),
                },
                HolderResponse {
                    address: HumanAddr::from("addr0002"),
                    balance: Uint128::from(300u128),
                    index: Decimal::zero(),
                    pending_rewards: Uint128::zero(),
                }
            ],
        }
    );

    // Set start_after and limit
    let res = query(
        &deps,
        QueryMsg::Holders {
            start_after: Some(HumanAddr::from("addr0000")),
            limit: Some(1),
        },
    )
    .unwrap();
    let holders_response: HoldersResponse = from_binary(&res).unwrap();
    assert_eq!(
        holders_response,
        HoldersResponse {
            holders: vec![HolderResponse {
                address: HumanAddr::from("addr0001"),
                balance: Uint128::from(200u128),
                index: Decimal::zero(),
                pending_rewards: Uint128::zero(),
            }],
        }
    );
}
