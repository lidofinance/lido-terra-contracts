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
use anchor_basset_validators_registry::msg::QueryMsg as QueryValidators;
use anchor_basset_validators_registry::registry::Validator as RegistryValidator;
use cosmwasm_std::{
    coin, coins, from_binary, to_binary, Api, BankMsg, Coin, CosmosMsg, Decimal, Env, Extern,
    FullDelegation, HandleResponse, HumanAddr, InitResponse, Querier, QueryRequest, StakingMsg,
    StdError, Storage, Uint128, Validator, WasmMsg, WasmQuery,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::mock_env;

use crate::msg::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, InitMsg, StateResponse,
    UnbondRequestsResponse, WithdrawableUnbondedResponse,
};
use hub_querier::{Cw20HookMsg, HandleMsg};

use crate::contract::{handle, init, query};
use crate::unbond::{handle_unbond, handle_unbond_stluna};

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use cw20_base::msg::HandleMsg::{Burn, Mint};
use hub_querier::Cw20HookMsg::Unbond;
use hub_querier::HandleMsg::{CheckSlashing, Receive, UpdateConfig, UpdateParams};

use super::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
use crate::math::decimal_division;
use crate::msg::QueryMsg::{
    AllHistory, Config, CurrentBatch, Parameters as Params, State, UnbondRequests,
    WithdrawableUnbonded,
};
use crate::state::{read_config, read_unbond_wait_list, store_state, Parameters};
use anchor_airdrop_registry::msg::HandleMsg::{FabricateANCClaim, FabricateMIRClaim};
use anchor_airdrop_registry::msg::PairHandleMsg;
use anchor_basset_rewards_dispatcher::msg::HandleMsg::{DispatchRewards, SwapToRewardDenom};

use cosmwasm_std::testing::{MockApi, MockStorage};

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2000";
const DEFAULT_VALIDATOR3: &str = "default-validator3000";

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub const _INITIAL_DEPOSIT_AMOUNT: Uint128 = Uint128(1000000u128);

fn sample_validator<U: Into<HumanAddr>>(addr: U) -> Validator {
    Validator {
        address: addr.into(),
        commission: Decimal::percent(3),
        max_commission: Decimal::percent(10),
        max_change_rate: Decimal::percent(1),
    }
}

fn set_validator_mock(querier: &mut WasmMockQuerier) {
    querier.update_staking(
        "uluna",
        &[
            sample_validator(DEFAULT_VALIDATOR),
            sample_validator(DEFAULT_VALIDATOR2),
            sample_validator(DEFAULT_VALIDATOR3),
        ],
        &[],
    );
}

pub fn initialize<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    owner: HumanAddr,
    reward_contract: HumanAddr,
    bluna_token_contract: HumanAddr,
    stluna_token_contract: HumanAddr,
) {
    let msg = InitMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let owner_env = mock_env(owner, &[]);
    init(&mut deps, owner_env.clone(), msg).unwrap();

    let register_msg = HandleMsg::UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: Some(reward_contract),
        bluna_token_contract: Some(bluna_token_contract),
        stluna_token_contract: Some(stluna_token_contract),
        airdrop_registry_contract: Some(HumanAddr::from("airdrop_registry")),
        validators_registry_contract: Some(HumanAddr::from("validators_registry")),
    };
    let res = handle(&mut deps, owner_env, register_msg).unwrap();
    assert_eq!(1, res.messages.len());
}

pub fn do_register_validator(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    validator: Validator,
) {
    deps.querier.add_validator(RegistryValidator {
        total_delegated: Uint128::zero(),
        address: validator.address,
    });
}

pub fn do_bond(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    addr: HumanAddr,
    amount: Uint128,
) {
    let validators: Vec<RegistryValidator> = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: HumanAddr::from("validators_registry"),
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {}).unwrap(),
        }))
        .unwrap();

    let bond = HandleMsg::Bond {};

    let env = mock_env(&addr, &[coin(amount.0, "uluna")]);
    let res = handle(deps, env, bond).unwrap();
    assert_eq!(validators.len() + 1, res.messages.len());
}

pub fn do_bond_stluna(
    deps: &mut Extern<MockStorage, MockApi, WasmMockQuerier>,
    addr: HumanAddr,
    amount: Uint128,
) {
    let validators: Vec<RegistryValidator> = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: HumanAddr::from("validators_registry"),
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {}).unwrap(),
        }))
        .unwrap();

    let bond = HandleMsg::BondForStLuna {};

    let env = mock_env(&addr, &[coin(amount.0, "uluna")]);
    let res = handle(deps, env, bond).unwrap();
    assert_eq!(validators.len() + 1, res.messages.len());
}

pub fn do_unbond<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr: HumanAddr,
    env: Env,
    amount: Uint128,
) -> HandleResponse {
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr,
        amount,
        msg: Some(to_binary(&successful_bond).unwrap()),
    });

    handle(&mut deps, env, receive).unwrap()
}

/// Covers if all the fields of InitMsg are stored in
/// parameters' storage, the config storage stores the creator,
/// the current batch storage and state are initialized.
#[test]
fn proper_initialization() {
    let mut deps = dependencies(20, &[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    // successful call
    let msg = InitMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 210,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let owner = HumanAddr::from("owner1");
    let owner_env = mock_env(owner, &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    // check parameters storage
    let params = Params {};
    let query_params: Parameters = from_binary(&query(&deps, params).unwrap()).unwrap();
    assert_eq!(query_params.epoch_period, 30);
    assert_eq!(query_params.underlying_coin_denom, "uluna");
    assert_eq!(query_params.unbonding_period, 210);
    assert_eq!(query_params.peg_recovery_fee, Decimal::zero());
    assert_eq!(query_params.er_threshold, Decimal::one());
    assert_eq!(query_params.reward_denom, "uusd");

    // state storage must be initialized
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    let expected_result = StateResponse {
        bluna_exchange_rate: Decimal::one(),
        stluna_exchange_rate: Decimal::one(),
        total_bond_bluna_amount: Uint128::zero(),
        total_bond_stluna_amount: Uint128::zero(),
        last_index_modification: owner_env.block.time,
        prev_hub_balance: Default::default(),
        actual_unbonded_amount: Default::default(),
        last_unbonded_time: owner_env.block.time,
        last_processed_batch: 0u64,
    };
    assert_eq!(query_state, expected_result);

    // config storage must be initialized
    let conf = Config {};
    let query_conf: ConfigResponse = from_binary(&query(&deps, conf).unwrap()).unwrap();
    let expected_conf = ConfigResponse {
        owner: HumanAddr::from("owner1"),
        reward_dispatcher_contract: None,
        validators_registry_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        stluna_token_contract: None,
    };

    assert_eq!(expected_conf, query_conf);

    // current branch storage must be initialized
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(
        query_batch,
        CurrentBatchResponse {
            id: 1,
            requested_bluna_with_fee: Default::default(),
            requested_stluna: Default::default()
        }
    );
}

#[test]
fn proper_bond() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10000);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = HandleMsg::Bond {};

    let env = mock_env(&addr1, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[3];
    match mint {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &HumanAddr::from("token"));
            assert_eq!(
                msg,
                &to_binary(&Cw20HandleMsg::Mint {
                    recipient: addr1.clone(),
                    amount: bond_amount,
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", mint),
    }

    // get total bonded
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_bluna_amount, bond_amount);
    assert_eq!(query_state.bluna_exchange_rate, Decimal::one());

    // no-send funds
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::Bond {};

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(10, "ukrt")]);
    let res = handle(&mut deps, env, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let env = mock_env(
        &addr1,
        &[coin(bond_amount.0, "uluna"), coin(bond_amount.0, "uusd")],
    );

    let res = handle(&mut deps, env, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );
}

#[test]
fn proper_bond_for_st_luna() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10000);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&addr1, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[3];
    match mint {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &stluna_token_contract);
            assert_eq!(
                msg,
                &to_binary(&Cw20HandleMsg::Mint {
                    recipient: addr1.clone(),
                    amount: bond_amount,
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", mint),
    }

    // get total bonded
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_stluna_amount, bond_amount);
    assert_eq!(query_state.stluna_exchange_rate, Decimal::one());

    // no-send funds
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(10, "ukrt")]);
    let res = handle(&mut deps, env, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let env = mock_env(
        &addr1,
        &[coin(bond_amount.0, "uluna"), coin(bond_amount.0, "uusd")],
    );

    let res = handle(&mut deps, env, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );
}

#[test]
fn proper_bond_rewards() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10000);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_dispatcher_contract = HumanAddr::from("reward_dispatcher");

    initialize(
        &mut deps,
        owner,
        reward_dispatcher_contract.clone(),
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&addr1, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&addr1, &bond_amount)])]);

    let bond_msg = HandleMsg::BondRewards {};

    let env = mock_env(&reward_dispatcher_contract, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, &coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    // get total bonded
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(
        query_state.total_bond_stluna_amount,
        bond_amount + bond_amount // BondForStLuna + BondRewards
    );
    assert_eq!(
        query_state.stluna_exchange_rate,
        Decimal::from_ratio(2u128, 1u128)
    );

    // no-send funds
    let failed_bond = HandleMsg::BondRewards {};

    let env = mock_env(&reward_dispatcher_contract, &[]);
    let res = handle(&mut deps, env, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let failed_bond = HandleMsg::BondRewards {};

    let env = mock_env(&reward_dispatcher_contract, &[coin(10, "ukrt")]);
    let res = handle(&mut deps, env, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let env = mock_env(
        &reward_dispatcher_contract,
        &[coin(bond_amount.0, "uluna"), coin(bond_amount.0, "uusd")],
    );

    let res = handle(&mut deps, env, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );

    //bond from non-dispatcher address
    let env = mock_env(
        &HumanAddr::from("random_address"),
        &[coin(bond_amount.0, "uluna")],
    );
    let failed_bond = HandleMsg::BondRewards {};

    let res = handle(&mut deps, env, failed_bond).unwrap_err();
    assert_eq!(res, StdError::unauthorized());
}

/// Covers if Withdraw message, swap message, and update global index are sent.
#[test]
pub fn proper_update_global_index() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract.clone(),
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &bond_amount)]),
    ]);

    // fails if there is no delegation
    let reward_msg = HandleMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let env = mock_env(&addr1, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    // bond
    do_bond(&mut deps, addr1.clone(), bond_amount);

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(bond_amount.0, "uluna")))];

    let validators: [Validator; 1] = [(validator.clone())];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &bond_amount)]),
    ]);

    let reward_msg = HandleMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let env = mock_env(&addr1, &[]);
    let res = handle(&mut deps, env.clone(), reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let last_index_query = State {};
    let last_modification: StateResponse =
        from_binary(&query(&deps, last_index_query).unwrap()).unwrap();
    assert_eq!(&last_modification.last_index_modification, &env.block.time);

    let withdraw = &res.messages[0];
    match withdraw {
        CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient,
        }) => {
            assert_eq!(val, &validator.address);
            assert_eq!(recipient.is_none(), true);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let swap = &res.messages[1];
    match swap {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(
                msg,
                &to_binary(&SwapToRewardDenom {
                    stluna_total_mint_amount: Uint128(10),
                    bluna_total_mint_amount: Uint128(10),
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", swap),
    }

    let update_g_index = &res.messages[2];
    match update_g_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(msg, &to_binary(&DispatchRewards {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", update_g_index),
    }
}

/// Covers update_global_index when there is more than one validator.
/// Checks if more than one Withdraw message is sent.
#[test]
pub fn proper_update_global_index_two_validators() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128(10));

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128(10))]),
    ]);

    // register_validator
    do_register_validator(&mut deps, validator2.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128(10));

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(10, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(10, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator.clone()), (validator2.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(20u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128(10))]),
    ]);

    let reward_msg = HandleMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let env = mock_env(&addr1, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(4, res.messages.len());

    let withdraw = &res.messages[0];
    match withdraw {
        CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient,
        }) => {
            assert_eq!(val, &validator.address);
            assert_eq!(recipient.is_none(), true);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let withdraw = &res.messages[1];
    match withdraw {
        CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient,
        }) => {
            assert_eq!(val, &validator2.address);
            assert_eq!(recipient.is_none(), true);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }
}

/// Covers update_global_index when more than on validator is registered, but
/// there is only a delegation to only one of them.
/// Checks if one Withdraw message is sent.
#[test]
pub fn proper_update_global_index_respect_one_registered_validator() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128(10));

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128(10))]),
    ]);

    // register_validator 2 but will not bond anything to it
    do_register_validator(&mut deps, validator2);

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(10, "uluna")))];

    let validators: [Validator; 1] = [(validator.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(20u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128(10))]),
    ]);

    let reward_msg = HandleMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let env = mock_env(&addr1, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let withdraw = &res.messages[0];
    match withdraw {
        CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient,
        }) => {
            assert_eq!(val, &validator.address);
            assert_eq!(recipient.is_none(), true);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }
}

/// Covers if the receive message is sent by token contract,
/// if handle_unbond is executed.
/*
    A comprehensive test for unbond is prepared in proper_unbond tests
*/
#[test]
pub fn proper_receive() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr0001");
    let invalid = HumanAddr::from("invalid");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128(10));
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))]),
        (&stluna_token_contract, &[]),
    ]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(10),
        msg: None,
    });

    let token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env, receive);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Invalid request: \"unbond\" message not included in request")
    );

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(10),
        msg: Some(to_binary(&failed_unbond).unwrap()),
    });

    let invalid_env = mock_env(&invalid, &[]);
    let res = handle(&mut deps, invalid_env, receive);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // successful call
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128(10),
        msg: Some(to_binary(&successful_unbond).unwrap()),
    });

    let valid_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, valid_env, receive).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg = &res.messages[0];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Burn {
                    amount: Uint128(10)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }
}

/// Covers if the epoch period is passed, Undelegate message is sent,
/// the state storage is updated to the new changed value,
/// the current epoch is updated to the new values,
/// the request is stored in unbond wait list, and unbond history map is updated
#[test]
pub fn proper_unbond() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let res = handle(&mut deps, env, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    //check the current batch before unbond
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    let token_env = mock_env(&token_contract, &[]);

    // check the state before unbond
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_bluna_amount, Uint128(10));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(1),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(9u128))]),
        (&stluna_token_contract, &[]),
    ]);

    //read the undelegated waitlist of the current epoch for the user bob
    let wait_list = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128(1), wait_list);

    //successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(5),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let mut token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(4u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let msg = &res.messages[0];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(msg, &to_binary(&Burn { amount: Uint128(5) }).unwrap());
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let waitlist2 = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128(6), waitlist2);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128(6));

    //pushing time forward to check the unbond message
    token_env.block.time += 31;

    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(2),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(2, res.messages.len());

    let msg = &res.messages[1];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(msg, &to_binary(&Burn { amount: Uint128(2) }).unwrap());
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    //making sure the sent message (2nd) is undelegate
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    });
    assert_eq!(res.messages[0], msgs);

    // check the current batch
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    // check the state
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_bluna_amount, Uint128(2));

    // the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = UnbondRequests { address: bob };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(&deps, waitlist).unwrap()).unwrap();
    assert_eq!(query_unbond.requests[0].0, 1);
    assert_eq!(query_unbond.requests[0].1, Uint128(8));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(8));
    assert_eq!(res.history[0].bluna_applied_exchange_rate, Decimal::one());
    assert_eq!(res.history[0].released, false);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the receive message is sent by token contract,
/// if handle_unbond is executed.
/*
    A comprehensive test for unbond is prepared in proper_unbond tests
*/
#[test]
pub fn proper_receive_stluna() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr0001");
    let invalid = HumanAddr::from("invalid");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond_stluna(&mut deps, addr1.clone(), Uint128(10));
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&addr1, &Uint128(10u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(10),
        msg: None,
    });

    let token_env = mock_env(&stluna_token_contract, &[]);
    let res = handle(&mut deps, token_env, receive);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Invalid request: \"unbond\" message not included in request")
    );

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(10),
        msg: Some(to_binary(&failed_unbond).unwrap()),
    });

    let invalid_env = mock_env(&invalid, &[]);
    let res = handle(&mut deps, invalid_env, receive);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // successful call
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128(10),
        msg: Some(to_binary(&successful_unbond).unwrap()),
    });

    let valid_env = mock_env(&stluna_token_contract, &[]);
    let res = handle(&mut deps, valid_env, receive).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg = &res.messages[0];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &stluna_token_contract);
            assert_eq!(
                msg,
                &to_binary(&Burn {
                    amount: Uint128(10)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }
}

/// Covers if the epoch period is passed, Undelegate message is sent,
/// the state storage is updated to the new changed value,
/// the current epoch is updated to the new values,
/// the request is stored in unbond wait list, and unbond history map is updated
#[test]
pub fn proper_unbond_stluna() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(10u128))]),
        (&token_contract, &[]),
    ]);

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    //check the current batch before unbond
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());
    assert_eq!(query_batch.requested_stluna, Uint128::zero());

    let token_env = mock_env(&stluna_token_contract, &[]);

    // check the state before unbond
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_stluna_amount, Uint128(10));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(1),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(9u128))]),
        (&token_contract, &[]),
    ]);

    //read the undelegated waitlist of the current epoch for the user bob
    let wait_list = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128(1), wait_list);

    //successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(5),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let mut token_env = mock_env(&stluna_token_contract, &[]);
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(4u128))]),
        (&token_contract, &[]),
    ]);

    let msg = &res.messages[0];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &stluna_token_contract);
            assert_eq!(msg, &to_binary(&Burn { amount: Uint128(5) }).unwrap());
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let waitlist2 = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128(6), waitlist2);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_stluna, Uint128(6));
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    //pushing time forward to check the unbond message
    token_env.block.time += 31;

    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(2),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(2, res.messages.len());

    let msg = &res.messages[1];
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &stluna_token_contract);
            assert_eq!(msg, &to_binary(&Burn { amount: Uint128(2) }).unwrap());
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    //making sure the sent message (2nd) is undelegate
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    });
    assert_eq!(res.messages[0], msgs);

    // check the current batch
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_stluna, Uint128::zero());
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    // check the state
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_bluna_amount, Uint128(0));
    assert_eq!(query_state.total_bond_stluna_amount, Uint128(2));

    // the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = UnbondRequests { address: bob };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(&deps, waitlist).unwrap()).unwrap();
    assert_eq!(query_unbond.requests[0].0, 1);
    assert_eq!(query_unbond.requests[0].1, Uint128(8));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128(8));
    assert_eq!(res.history[0].stluna_applied_exchange_rate, Decimal::one());
    assert_eq!(res.history[0].released, false);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the pick_validator function sends different Undelegate messages
/// to different validators, when a validator does not have enough delegation.
#[test]
pub fn proper_pick_validator() {
    let mut deps = dependencies(20, &[]);

    let addr1 = HumanAddr::from("addr1000");
    let addr2 = HumanAddr::from("addr2000");
    let addr3 = HumanAddr::from("addr3000");

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);

    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3.clone());

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128(10));
    do_bond(&mut deps, addr2.clone(), Uint128(150));
    do_bond(&mut deps, addr3.clone(), Uint128(200));

    // give validators different delegation amount
    let delegations: [FullDelegation; 3] = [
        (sample_delegation(validator.address.clone(), coin(10, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(150, "uluna"))),
        (sample_delegation(validator3.address.clone(), coin(200, "uluna"))),
    ];

    let validators: [Validator; 3] = [(validator), (validator2.clone()), (validator3.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);
    deps.querier.with_token_balances(&[
        (
            &HumanAddr::from("token"),
            &[
                (&addr3, &Uint128(200)),
                (&addr2, &Uint128(150)),
                (&addr1, &Uint128(10)),
            ],
        ),
        (&stluna_token_contract, &[]),
    ]);

    // send the first burn
    let mut token_env = mock_env(&token_contract, &[]);
    let res = do_unbond(&mut deps, addr2.clone(), token_env.clone(), Uint128(50));
    assert_eq!(res.messages.len(), 1);

    deps.querier.with_token_balances(&[
        (
            &HumanAddr::from("token"),
            &[
                (&addr3, &Uint128(200)),
                (&addr2, &Uint128(100)),
                (&addr1, &Uint128(10)),
            ],
        ),
        (&stluna_token_contract, &[]),
    ]);

    token_env.block.time += 40;

    // send the second burn
    let res = do_unbond(&mut deps, addr2.clone(), token_env, Uint128(100));
    assert_eq!(res.messages.len(), 3);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[
            (&addr3, &Uint128(200)),
            (&addr2, &Uint128(0)),
            (&addr1, &Uint128(10)),
        ],
    )]);

    //check if the undelegate message is send two more than one validator.
    match &res.messages[0] {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(val, &validator3.address);
            assert_eq!(amount.amount, Uint128(130));
        }
        _ => panic!("Unexpected message: {:?}", &res.messages[0]),
    }
    match &res.messages[1] {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(val, &validator2.address);
            assert_eq!(amount.amount, Uint128(20));
        }
        _ => panic!("Unexpected message: {:?}", &res.messages[0]),
    }
}

/// Covers if the pick_validator function sends different Undelegate messages
/// if the delegations of the user are distributed to several validators
/// and if the user wants to unbond amount that none of validators has.
#[test]
pub fn proper_pick_validator_respect_distributed_delegation() {
    let mut deps = dependencies(20, &[]);

    let addr1 = HumanAddr::from("addr1000");
    let addr2 = HumanAddr::from("addr2000");

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);

    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3);

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128(1000));
    do_bond(&mut deps, addr1.clone(), Uint128(1500));

    // give validators different delegation amount
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(1000, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(1500, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator), (validator2)];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(2500))]),
        (&stluna_token_contract, &[]),
    ]);

    // send the first burn
    let mut token_env = mock_env(&token_contract, &[]);

    token_env.block.time += 40;

    let res = do_unbond(&mut deps, addr2, token_env, Uint128(2000));
    assert_eq!(res.messages.len(), 3);

    match &res.messages[0] {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: _,
            amount,
        }) => assert_eq!(amount.amount, Uint128(1250)),
        _ => panic!("Unexpected message: {:?}", &res.messages[1]),
    }
}

/// Covers the effect of slashing of bond, unbond, and withdraw_unbonded
/// update the exchange rate after and before slashing.
#[test]
pub fn proper_slashing() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");
    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //bond
    do_bond(&mut deps, addr1.clone(), Uint128(1000));

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(1000u128))]),
        (&stluna_token_contract, &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let env = mock_env(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate.to_string(), "0.9");

    //bond again to see the update exchange rate
    let second_bond = HandleMsg::Bond {};

    let env = mock_env(&addr1, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    // expected exchange rate must be more than 0.9
    let expected_er = Decimal::from_ratio(Uint128(1900), Uint128(2111));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(1000, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let message = &res.messages[1];
    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Mint {
                    recipient: env.message.sender,
                    amount: Uint128(1111)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    set_delegation(&mut deps.querier, validator.clone(), 100900, "uluna");

    //update user balance
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(2111u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let mut env = mock_env(&addr1, &[]);
    let _res = handle_unbond(&mut deps, env.clone(), Uint128(500), addr1.clone()).unwrap();

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &Uint128(1611u128))]),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time += 31;
    let res = handle_unbond(&mut deps, env.clone(), Uint128(500), addr1.clone()).unwrap();
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(900, "uluna"),
    });
    assert_eq!(res.messages[0], msgs);

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(1111u128))])]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    env.block.time += 90;
    //check withdrawUnbonded message
    let withdraw_unbond_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    let sent_message = &wdraw_unbonded_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &addr1);
            assert_eq!(amount[0].amount, Uint128(900))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }
}

/// Covers the effect of slashing of bond, unbond, and withdraw_unbonded
/// update the exchange rate after and before slashing.
#[test]
pub fn proper_slashing_stluna() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");
    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //bond
    do_bond_stluna(&mut deps, addr1.clone(), Uint128(1000));

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&addr1, &Uint128(1000u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let env = mock_env(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.stluna_exchange_rate.to_string(), "0.9");

    //bond again to see the update exchange rate
    let second_bond = HandleMsg::BondForStLuna {};

    let env = mock_env(&addr1, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    // expected exchange rate must be more than 0.9
    let expected_er = Decimal::from_ratio(Uint128(1900), Uint128(2111));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.stluna_exchange_rate, expected_er);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(1000, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let message = &res.messages[1];
    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &stluna_token_contract);
            assert_eq!(
                msg,
                &to_binary(&Mint {
                    recipient: env.message.sender,
                    amount: Uint128(1111)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    set_delegation(&mut deps.querier, validator.clone(), 100900, "uluna");

    //update user balance
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&addr1, &Uint128(2111u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    let mut env = mock_env(&addr1, &[]);
    let _res = handle_unbond_stluna(&mut deps, env.clone(), Uint128(500), addr1.clone()).unwrap();

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&addr1, &Uint128(1611u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    env.block.time += 31;
    let res = handle_unbond_stluna(&mut deps, env.clone(), Uint128(500), addr1.clone()).unwrap();
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(900, "uluna"),
    });
    assert_eq!(res.messages[0], msgs);

    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&addr1, &Uint128(1111u128))])]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);

    let expected_er = Decimal::from_ratio(Uint128(1000), Uint128(1111));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.stluna_exchange_rate, expected_er);

    env.block.time += 90;
    //check withdrawUnbonded message
    let withdraw_unbond_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.stluna_exchange_rate, expected_er);

    let sent_message = &wdraw_unbonded_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &addr1);
            assert_eq!(amount[0].amount, Uint128(900))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }
}

/// Covers if the withdraw_rate function is updated before and after withdraw_unbonded,
/// the finished amount is accurate, user requests are removed from the waitlist, and
/// the BankMsg::Send is sent.
#[test]
pub fn proper_withdraw_unbonded() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(100u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, 100, "uluna");

    let res = handle_unbond(&mut deps, env, Uint128(10), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(90u128))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.
    env.block.time += 31;

    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());

    // trigger undelegation message
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    let res = handle_unbond(&mut deps, env.clone(), Uint128(10), bob.clone()).unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(80u128))]),
        (&stluna_token_contract, &[]),
    ]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 91;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(20),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(20));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(20));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(20));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(20))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(&deps, waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128(0));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
}

/// Covers if the withdraw_rate function is updated before and after withdraw_unbonded,
/// the finished amount is accurate, user requests are removed from the waitlist, and
/// the BankMsg::Send is sent.
#[test]
pub fn proper_withdraw_unbonded_stluna() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(100, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(100u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let bond_msg = HandleMsg::BondRewards {};

    let env = mock_env(&HumanAddr::from("reward"), &[coin(100, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(1, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 200, "uluna");

    let res = handle_unbond_stluna(&mut deps, env, Uint128(10), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(90u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.
    env.block.time += 31;

    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());

    // trigger undelegation message
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    let res = handle_unbond_stluna(&mut deps, env.clone(), Uint128(10), bob.clone()).unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(80u128))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_stluna_amount, Uint128::from(160u64));
    assert_eq!(
        query_state.stluna_exchange_rate,
        Decimal::from_ratio(2u128, 1u128)
    );

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 91;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(40),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(20));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128(20));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(40));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(40))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(&deps, waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128(0));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
}

#[test]
pub fn proper_withdraw_unbonded_both_tokens() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};
    let bond_for_stluna_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contracts
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(100u128))]),
        (&stluna_token_contract, &[(&bob, &Uint128(100u128))]),
    ]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 100, "uluna");

    let res = handle(&mut deps, env.clone(), bond_for_stluna_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 200, "uluna");

    let bond_msg = HandleMsg::BondRewards {};

    let env = mock_env(&HumanAddr::from("reward"), &[coin(100, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(1, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 300, "uluna");

    let res = handle_unbond(&mut deps, env.clone(), Uint128(100), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let mut env = mock_env(&bob, &[]);
    env.block.time += 31;
    let res = handle_unbond_stluna(&mut deps, env.clone(), Uint128(100), bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(0u128))]),
        (&stluna_token_contract, &[(&bob, &Uint128(0u128))]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 91;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(300),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(200));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(100));
    assert_eq!(res.history[0].stluna_amount, Uint128(100));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(300));

    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(298)) // not 300 because of decimal
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(&deps, waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128(2));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
    assert_eq!(state_query.stluna_exchange_rate, Decimal::one());
}

/// Covers slashing during the unbonded period and its effect on the finished amount.
#[test]
pub fn proper_withdraw_unbonded_respect_slashing() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(bond_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.0, "uluna");

    let res = handle_unbond(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(9500))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    env.block.time += 31;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9000))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 91;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);

    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(1000));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));
}

/// Covers slashing during the unbonded period and its effect on the finished amount.
#[test]
pub fn proper_withdraw_unbonded_respect_slashing_stluna() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&HumanAddr::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(bond_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.0, "uluna");

    let res = handle_unbond_stluna(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(9500))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    env.block.time += 31;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = handle_unbond_stluna(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&bob, &Uint128(9000))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 91;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);

    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(1000));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));
}

/// Covers withdraw_unbonded/inactivity in the system while there are slashing events.
#[test]
pub fn proper_withdraw_unbonded_respect_inactivity_slashing() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(bond_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.0, "uluna");

    let res = handle_unbond(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(9500))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, unbond_amount);

    env.block.time += 1000;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9000))])]);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(1000));
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "1");
    assert_eq!(res.history[0].released, false);
    assert_eq!(res.history[0].batch_id, 1);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::zero());

    env.block.time += 1091;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(1000));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(1000));
    assert_eq!(res.history[0].bluna_applied_exchange_rate.to_string(), "1");
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "0.899");
    assert_eq!(res.history[0].released, true);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers withdraw_unbonded/inactivity in the system while there are slashing events.
#[test]
pub fn proper_withdraw_unbonded_respect_inactivity_slashing_stluna() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&HumanAddr::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(bond_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.0, "uluna");

    let res = handle_unbond_stluna(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(9500))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_stluna, unbond_amount);

    env.block.time += 1000;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = handle_unbond_stluna(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&bob, &Uint128(9000))])]);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_stluna, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128(1000));
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "1");
    assert_eq!(res.history[0].released, false);
    assert_eq!(res.history[0].batch_id, 1);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::zero());

    env.block.time += 1091;

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(900),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(1000));

    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128(1000));
    assert_eq!(res.history[0].stluna_applied_exchange_rate.to_string(), "1");
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "0.899");
    assert_eq!(res.history[0].released, true);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the signed integer works properly,
/// the exception when a user sends rogue coin.
#[test]
pub fn proper_withdraw_unbond_with_dummies() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), bond_amount.0, "uluna");

    let res = handle_unbond(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(9500))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    env.block.time += 31;
    // trigger undelegation message
    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(9000))]),
        (&stluna_token_contract, &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator, bond_amount.0 - 2000, "uluna");

    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(8500))]),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time += 31;
    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &Uint128(8000))]),
        (&stluna_token_contract, &[]),
    ]);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(2200),
        },
    )]);

    env.block.time += 120;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128(1000));
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "1.164");
    assert_eq!(res.history[0].released, true);
    assert_eq!(res.history[0].batch_id, 1);
    assert_eq!(res.history[1].bluna_amount, Uint128(1000));
    assert_eq!(res.history[1].bluna_withdraw_rate.to_string(), "1.033");
    assert_eq!(res.history[1].released, true);
    assert_eq!(res.history[1].batch_id, 2);

    let expected = (res.history[0].bluna_withdraw_rate * res.history[0].bluna_amount)
        + res.history[1].bluna_withdraw_rate * res.history[1].bluna_amount;
    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, expected)
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));
}

/// Covers if the signed integer works properly,
/// the exception when a user sends rogue coin.
#[test]
pub fn proper_withdraw_unbond_with_dummies_stluna() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128(10000);
    let unbond_amount = Uint128(500);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::BondForStLuna {};

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&HumanAddr::from("token"), &[]),
    ]);

    set_delegation(&mut deps.querier, validator.clone(), bond_amount.0, "uluna");

    let res = handle_unbond_stluna(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(9500))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(0),
        },
    )]);

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.

    env.block.time += 31;
    // trigger undelegation message
    let res = handle_unbond_stluna(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(9000))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator, bond_amount.0 - 2000, "uluna");

    let res = handle_unbond_stluna(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(8500))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    env.block.time += 31;
    let res = handle_unbond_stluna(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128(8000))]),
        (&HumanAddr::from("token"), &[]),
    ]);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(2200),
        },
    )]);

    env.block.time += 120;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let success_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128(1000));
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "1.164");
    assert_eq!(res.history[0].released, true);
    assert_eq!(res.history[0].batch_id, 1);
    assert_eq!(res.history[1].stluna_amount, Uint128(1000));
    assert_eq!(res.history[1].stluna_withdraw_rate.to_string(), "1.033");
    assert_eq!(res.history[1].released, true);
    assert_eq!(res.history[1].batch_id, 2);

    let expected = (res.history[0].stluna_withdraw_rate * res.history[0].stluna_amount)
        + res.history[1].stluna_withdraw_rate * res.history[1].stluna_amount;
    let sent_message = &success_res.messages[0];
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, expected)
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(&deps, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128(0));
}

/// Covers if the state/parameters storage is updated to the given value,
/// who sends the message, and if
/// RewardUpdateDenom message is sent to the reward contract
#[test]
pub fn test_update_params() {
    let mut deps = dependencies(20, &[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    //test with no swap denom.
    let update_prams = UpdateParams {
        epoch_period: Some(20),
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, update_prams.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters = from_binary(&query(&deps, Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_period, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.unbonding_period, 2);
    assert_eq!(params.peg_recovery_fee, Decimal::zero());
    assert_eq!(params.er_threshold, Decimal::one());
    assert_eq!(params.reward_denom, "uusd");

    //test with some swap_denom.
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: Some(3),
        peg_recovery_fee: Some(Decimal::one()),
        er_threshold: Some(Decimal::zero()),
    };

    //the result must be 1
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters = from_binary(&query(&deps, Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_period, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.unbonding_period, 3);
    assert_eq!(params.peg_recovery_fee, Decimal::one());
    assert_eq!(params.er_threshold, Decimal::zero());
    assert_eq!(params.reward_denom, "uusd");
}

/// Covers if peg recovery is applied (in "bond", "unbond",
/// and "withdraw_unbonded" messages) in case of a slashing event
#[test]
pub fn proper_recovery_fee() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: Some(Decimal::from_ratio(Uint128(1), Uint128(1000))),
        er_threshold: Some(Decimal::from_ratio(Uint128(99), Uint128(100))),
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    let bond_amount = Uint128(1000000u128);
    let unbond_amount = Uint128(100000u128);

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let get_params = Params {};
    let parmas: Parameters = from_binary(&query(&deps, get_params).unwrap()).unwrap();
    assert_eq!(parmas.epoch_period, 30);
    assert_eq!(parmas.underlying_coin_denom, "uluna");
    assert_eq!(parmas.unbonding_period, 2);
    assert_eq!(parmas.peg_recovery_fee.to_string(), "0.001");
    assert_eq!(parmas.er_threshold.to_string(), "0.99");

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 900000, "uluna");

    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate.to_string(), "0.9");

    //Bond again to see the applied result
    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {};

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    let mint_amount = decimal_division(bond_amount, Decimal::from_ratio(Uint128(9), Uint128(10)));
    let max_peg_fee = mint_amount * parmas.peg_recovery_fee;
    let required_peg_fee =
        ((bond_amount + mint_amount + Uint128::zero()) - (Uint128(900000) + bond_amount)).unwrap();
    let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
    let mint_amount_with_fee = (mint_amount - peg_fee).unwrap();

    let mint_msg = &res.messages[1];
    match mint_msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: _,
            msg,
            send: _,
        }) => assert_eq!(
            msg,
            &to_binary(&Mint {
                recipient: bob.clone(),
                amount: mint_amount_with_fee,
            })
            .unwrap()
        ),
        _ => panic!("Unexpected message: {:?}", mint_msg),
    }

    // check unbond message
    let unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract.clone(),
        amount: unbond_amount,
        msg: Some(to_binary(&unbond).unwrap()),
    });

    let new_balance = (bond_amount - unbond_amount).unwrap();

    let mut token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());

    //check current batch
    let bonded_with_fee = unbond_amount * Decimal::from_ratio(Uint128(999), Uint128(1000));
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(&deps, current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, bonded_with_fee);

    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&bob, &new_balance)]),
        (&stluna_token_contract, &[]),
    ]);

    token_env.block.time += 60;

    let second_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract,
        amount: unbond_amount,
        msg: Some(to_binary(&second_unbond).unwrap()),
    });
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(2, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    let new_exchange = query_exchange_rate.bluna_exchange_rate;

    let expected = bonded_with_fee + bonded_with_fee;
    let undelegate_message = &res.messages[0];
    match undelegate_message {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(&validator.address, val);
            assert_eq!(amount.amount, expected * new_exchange);
        }
        _ => panic!("Unexpected message: {:?}", mint_msg),
    }

    //got slashed during unbonding
    deps.querier.with_native_balances(&[(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128(161870),
        },
    )]);

    token_env.block.time += 90;
    //check withdrawUnbonded message
    let withdraw_unbond_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, token_env, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let sent_message = &wdraw_unbonded_res.messages[0];
    let expected =
        ((expected * new_exchange * Decimal::from_ratio(Uint128(161870), expected * new_exchange))
            - Uint128(1))
        .unwrap();
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send {
            from_address,
            to_address: _,
            amount,
        }) => {
            assert_eq!(from_address.0, MOCK_CONTRACT_ADDR);
            assert_eq!(amount[0].amount, expected);
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    // amount should be 99 + 99 since we store the requested amount with peg fee applied.
    assert_eq!(
        res.history[0].bluna_amount,
        bonded_with_fee + bonded_with_fee
    );
    assert_eq!(res.history[0].bluna_applied_exchange_rate, new_exchange);
    assert_eq!(
        res.history[0].bluna_withdraw_rate,
        Decimal::from_ratio(Uint128(161869), bonded_with_fee + bonded_with_fee)
    );
    assert_eq!(res.history[0].released, true);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the storage affected by update_config are updated properly
#[test]
pub fn proper_update_config() {
    let mut deps = dependencies(20, &[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let new_owner = HumanAddr::from("new_owner");
    let invalid_owner = HumanAddr::from("invalid_owner");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");
    let airdrop_registry = HumanAddr::from("airdrop_registry");

    initialize(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(&config_query.bluna_token_contract.unwrap(), &token_contract);
    assert_eq!(
        &config_query.airdrop_registry_contract.unwrap(),
        &airdrop_registry
    );

    //make sure the other configs are still the same.
    assert_eq!(
        &config_query.reward_dispatcher_contract.unwrap(),
        &reward_contract
    );
    assert_eq!(&config_query.owner, &owner);

    // only the owner can call this message
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let env = mock_env(&invalid_owner, &[]);
    let res = handle(&mut deps, env, update_config);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // change the owner
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = read_config(&deps.storage).load().unwrap();
    let new_owner_raw = deps.api.canonical_address(&new_owner).unwrap();
    assert_eq!(new_owner_raw, config.creator);

    // new owner can send the owner related messages
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
    };

    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    //previous owner cannot send this message
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
    };

    let new_owner_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: Some(HumanAddr::from("new reward")),
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
        validator: HumanAddr::default(),
        recipient: Some(HumanAddr::from("new reward")),
    });
    assert_eq!(msg, res.messages[0]);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.reward_dispatcher_contract.unwrap(),
        HumanAddr::from("new reward")
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        bluna_token_contract: Some(HumanAddr::from("new token")),
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.bluna_token_contract.unwrap(),
        HumanAddr::from("new token")
    );

    //make sure the other configs are still the same.
    assert_eq!(
        config_query.reward_dispatcher_contract.unwrap(),
        HumanAddr::from("new reward")
    );
    assert_eq!(config_query.owner, new_owner);

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: Some(HumanAddr::from("new airdrop")),
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.airdrop_registry_contract.unwrap(),
        HumanAddr::from("new airdrop")
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: Some(HumanAddr::from("new registry")),
        bluna_token_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.validators_registry_contract.unwrap(),
        HumanAddr::from("new registry"),
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        bluna_token_contract: None,
        stluna_token_contract: Some(stluna_token_contract.clone()),
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.stluna_token_contract.unwrap(),
        stluna_token_contract,
    );
}

#[test]
fn proper_claim_airdrop() {
    let mut deps = dependencies(20, &[]);

    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");
    let airdrop_registry = HumanAddr::from("airdrop_registry");

    initialize(
        &mut deps,
        owner.clone(),
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    let claim_msg = HandleMsg::ClaimAirdrop {
        airdrop_token_contract: HumanAddr::from("airdrop_token"),
        airdrop_contract: HumanAddr::from("MIR_contract"),
        airdrop_swap_contract: HumanAddr::from("airdrop_swap"),
        claim_msg: to_binary(&MIRMsg::MIRClaim {}).unwrap(),
        swap_msg: Default::default(),
    };

    //invalid sender
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, claim_msg.clone()).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err(format!("Sender must be {}", &airdrop_registry))
    );

    let valid_env = mock_env(&airdrop_registry, &[]);
    let res = handle(&mut deps, valid_env.clone(), claim_msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("MIR_contract"),
            msg: to_binary(&MIRMsg::MIRClaim {}).unwrap(),
            send: vec![],
        })
    );
    assert_eq!(
        res.messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: valid_env.contract.address,
            msg: to_binary(&HandleMsg::SwapHook {
                airdrop_token_contract: HumanAddr::from("airdrop_token"),
                airdrop_swap_contract: HumanAddr::from("airdrop_swap"),
                swap_msg: Default::default(),
            })
            .unwrap(),
            send: vec![],
        })
    );
}

#[test]
fn proper_swap_hook() {
    let mut deps = dependencies(20, &[]);

    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract,
        stluna_token_contract,
    );

    let swap_msg = HandleMsg::SwapHook {
        airdrop_token_contract: HumanAddr::from("airdrop_token"),
        airdrop_swap_contract: HumanAddr::from("swap_contract"),
        swap_msg: to_binary(&PairHandleMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: Some(reward_contract.clone()),
        })
        .unwrap(),
    };

    //invalid sender
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env.clone(), swap_msg.clone()).unwrap_err();
    assert_eq!(res, StdError::unauthorized());

    // no balance for hub
    let contract_env = mock_env(&env.contract.address, &[]);
    let res = handle(&mut deps, contract_env.clone(), swap_msg.clone()).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err(format!(
            "There is no balance for {} in airdrop token contract {}",
            &env.contract.address,
            &HumanAddr::from("airdrop_token")
        ))
    );

    deps.querier.with_token_balances(&[(
        &HumanAddr::from("airdrop_token"),
        &[(&env.contract.address, &Uint128(1000))],
    )]);

    let res = handle(&mut deps, contract_env, swap_msg).unwrap();
    assert_eq!(res.messages.len(), 1);
    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("airdrop_token"),
            msg: to_binary(&Cw20HandleMsg::Send {
                contract: HumanAddr::from("swap_contract"),
                amount: Uint128(1000),
                msg: Some(
                    to_binary(&PairHandleMsg::Swap {
                        belief_price: None,
                        max_spread: None,
                        to: Some(reward_contract),
                    })
                    .unwrap()
                ),
            })
            .unwrap(),
            send: vec![],
        })
    )
}

#[test]
fn proper_update_global_index_with_airdrop() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), bond_amount);

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(bond_amount.0, "uluna")))];

    let validators: [Validator; 1] = [(validator)];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&HumanAddr::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &Uint128(10))]),
    ]);

    let binary_msg = to_binary(&FabricateMIRClaim {
        stage: 0,
        amount: Uint128(1000),
        proof: vec!["proof".to_string()],
    })
    .unwrap();

    let binary_msg2 = to_binary(&FabricateANCClaim {
        stage: 0,
        amount: Uint128(1000),
        proof: vec!["proof".to_string()],
    })
    .unwrap();
    let reward_msg = HandleMsg::UpdateGlobalIndex {
        airdrop_hooks: Some(vec![binary_msg.clone(), binary_msg2.clone()]),
    };

    let env = mock_env(&addr1, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(5, res.messages.len());

    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("airdrop_registry"),
            msg: binary_msg,
            send: vec![],
        })
    );

    assert_eq!(
        res.messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("airdrop_registry"),
            msg: binary_msg2,
            send: vec![],
        })
    );
}

fn set_delegation(querier: &mut WasmMockQuerier, validator: Validator, amount: u128, denom: &str) {
    querier.update_staking(
        "uluna",
        &[validator.clone()],
        &[sample_delegation(validator.address, coin(amount, denom))],
    );
}

fn set_delegation_query(
    querier: &mut WasmMockQuerier,
    delegate: &[FullDelegation],
    validators: &[Validator],
) {
    querier.update_staking("uluna", validators, delegate);
}

fn sample_delegation(addr: HumanAddr, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = coin(0, &amount.denom);
    FullDelegation {
        validator: addr,
        delegator: HumanAddr::from(MOCK_CONTRACT_ADDR),
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}

// sample MIR claim msg
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MIRMsg {
    MIRClaim {},
}

#[test]
fn test_receive_cw20() {
    let mut deps = dependencies(20, &coins(2, "token"));
    // let stluna_addr = HumanAddr::from("stluna_token");
    let sender_addr = HumanAddr::from("addr001");
    let owner = HumanAddr::from("owner1");
    let bluna_token_contract = HumanAddr::from("token");
    let stluna_token_contract = HumanAddr::from("stluna_token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner,
        reward_contract,
        bluna_token_contract.clone(),
        stluna_token_contract.clone(),
    );
    store_state(&mut deps.storage)
        .update(|mut prev_state| {
            prev_state.total_bond_stluna_amount = Uint128(1000);
            prev_state.total_bond_bluna_amount = Uint128(1000);
            Ok(prev_state)
        })
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &HumanAddr::from("stluna_token"),
            &[(&sender_addr, &Uint128(1000))],
        ),
        (&HumanAddr::from("token"), &[(&sender_addr, &Uint128(1000))]),
    ]);
    {
        // just enough stluna tokens to convert
        let env = mock_env(stluna_token_contract.clone(), &[]);
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128(1000),
            msg: Some(to_binary(&Cw20HookMsg::Convert {}).unwrap()),
        });
        let _ = handle(&mut deps, env, msg).unwrap();
    }
    {
        // does not enough stluna tokens to convert
        let env = mock_env(stluna_token_contract, &[]);
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128(1001),
            msg: Some(to_binary(&Cw20HookMsg::Convert {}).unwrap()),
        });
        let err = handle(&mut deps, env, msg).unwrap_err();
        assert_eq!(
            StdError::generic_err(
                "Decrease amount cannot exceed total stluna bond amount: 0. Trying to reduce: 1001"
            ),
            err
        );
    }

    {
        // just enough bluna tokens to convert
        let env = mock_env(bluna_token_contract.clone(), &[]);
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128(1000),
            msg: Some(to_binary(&Cw20HookMsg::Convert {}).unwrap()),
        });
        let _ = handle(&mut deps, env, msg).unwrap();
    }
    {
        // does not enough bluna tokens to convert
        let env = mock_env(bluna_token_contract, &[]);
        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr,
            amount: Uint128(1001),
            msg: Some(to_binary(&Cw20HookMsg::Convert {}).unwrap()),
        });
        let err = handle(&mut deps, env, msg).unwrap_err();
        assert_eq!(
            StdError::generic_err(
                "Decrease amount cannot exceed total bluna bond amount: 1000. Trying to reduce: 1001"
            ),
            err
        );
    }
}
