// Copyright 2021 Anchor Protocol. Modified by Lido
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This integration test tries to run and call the generated wasm.
//! It depends on a Wasm build being available, which you can create with `cargo wasm`.
//! Then running `cargo integration-test` will validate we can properly call into that generated Wasm.
//!
//! You can easily convert unit tests to integration tests as follows:
//! 1. Copy them over verbatim
//! 2. Then change
//!      let mut deps = mock_dependencies &[]);
//!    to
//!      let mut deps = mock_instance(WASM, &[]);
//! 3. If you access raw storage, where ever you see something like:
//!      deps.storage.get(CONFIG_KEY).expect("no data stored");
//!    replace it with:
//!      deps.with_storage(|store| {
//!          let data = store.get(CONFIG_KEY).expect("no data stored");
//!          //...
//!      });
//! 4. Anywhere you see query(deps.as_ref(), ...) you must replace it with query(deps.as_mut(), ...)
use anchor_basset_validators_registry::msg::QueryMsg as QueryValidators;
use anchor_basset_validators_registry::registry::ValidatorResponse as RegistryValidator;
use cosmwasm_std::{
    coin, coins, from_binary, to_binary, Addr, Api, BankMsg, Coin, CosmosMsg, Decimal, DepsMut,
    DistributionMsg, Env, FullDelegation, MessageInfo, OwnedDeps, Querier, QueryRequest, Response,
    StakingMsg, StdError, StdResult, Storage, Uint128, Validator, WasmMsg, WasmQuery,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{mock_env, mock_info};

use crate::contract::{execute, instantiate, query};
use crate::unbond::{execute_unbond, execute_unbond_stluna};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw20_base::msg::ExecuteMsg::{Burn, Mint};

use super::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
use crate::math::decimal_division;
use crate::state::{read_unbond_wait_list, CONFIG, PARAMETERS, STATE};
use anchor_basset_rewards_dispatcher::msg::ExecuteMsg::{DispatchRewards, SwapToRewardDenom};
use basset::airdrop::PairHandleMsg;

use basset::airdrop::ExecuteMsg::{FabricateANCClaim, FabricateMIRClaim};
use basset::hub::Cw20HookMsg::Unbond;
use basset::hub::ExecuteMsg::{CheckSlashing, Receive, UpdateConfig, UpdateParams};
use basset::hub::QueryMsg::{
    AllHistory, Config, CurrentBatch, Parameters as Params, State, UnbondRequests,
    WithdrawableUnbonded,
};
use basset::hub::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, Cw20HookMsg, ExecuteMsg,
    InstantiateMsg, Parameters, StateResponse, UnbondRequestsResponse,
    WithdrawableUnbondedResponse,
};
use cosmwasm_std::testing::{MockApi, MockStorage};
use std::borrow::BorrowMut;
use std::str::FromStr;

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2000";
const DEFAULT_VALIDATOR3: &str = "default-validator3000";

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

//pub const _INITIAL_DEPOSIT_AMOUNT: Uint128 = Uint128::from(1000000u128);

fn sample_validator<U: Into<String>>(addr: U) -> Validator {
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
    deps: &mut OwnedDeps<S, A, Q>,
    owner: String,
    reward_contract: String,
    bluna_token_contract: String,
    stluna_token_contract: String,
) {
    let msg = InstantiateMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let owner_info = mock_info(owner.as_str(), &[]);
    instantiate(deps.as_mut(), mock_env(), owner_info.clone(), msg).unwrap();

    let register_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: Some(reward_contract),
        bluna_token_contract: Some(bluna_token_contract),
        stluna_token_contract: Some(stluna_token_contract),
        airdrop_registry_contract: Some(String::from("airdrop_registry")),
        validators_registry_contract: Some(String::from("validators_registry")),
    };
    let res = execute(deps.as_mut(), mock_env(), owner_info, register_msg).unwrap();
    assert_eq!(1, res.messages.len());
}

pub fn do_register_validator(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    validator: Validator,
) {
    deps.querier.add_validator(RegistryValidator {
        total_delegated: Default::default(),
        address: validator.address,
    });
}

pub fn do_bond(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    addr: String,
    amount: Uint128,
) {
    let validators: Vec<RegistryValidator> = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: String::from("validators_registry"),
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {}).unwrap(),
        }))
        .unwrap();

    let bond = ExecuteMsg::Bond {};

    let info = mock_info(&addr, &[coin(amount.u128(), "uluna")]);
    let res = execute(deps.as_mut(), mock_env(), info, bond).unwrap();
    assert_eq!(validators.len() + 1, res.messages.len());
}

pub fn do_bond_stluna(
    deps: &mut OwnedDeps<MockStorage, MockApi, WasmMockQuerier>,
    addr: String,
    amount: Uint128,
) {
    let validators: Vec<RegistryValidator> = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: String::from("validators_registry"),
            msg: to_binary(&QueryValidators::GetValidatorsForDelegation {}).unwrap(),
        }))
        .unwrap();

    let bond = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&addr, &[coin(amount.u128(), "uluna")]);
    let res = execute(deps.as_mut(), mock_env(), info, bond).unwrap();
    assert_eq!(validators.len() + 1, res.messages.len());
}

pub fn do_unbond(
    deps: DepsMut,
    addr: String,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Response {
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr,
        amount,
        msg: to_binary(&successful_bond).unwrap(),
    });

    execute(deps, env, info, receive).unwrap()
}

/// Covers if all the fields of InstantiateMsg are stored in
/// parameters' storage, the config storage stores the creator,
/// the current batch storage and state are initialized.
#[test]
fn proper_initialization() {
    let mut deps = dependencies(&[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    // successful call
    let msg = InstantiateMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 210,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let owner = String::from("owner1");
    let owner_info = mock_info(owner.as_str(), &[]);
    let env = mock_env();

    // we can just call .unwrap() to assert this was a success
    let res: Response = instantiate(deps.as_mut(), mock_env(), owner_info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // check parameters storage
    let params = Params {};
    let query_params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), params).unwrap()).unwrap();
    assert_eq!(query_params.epoch_period, 30);
    assert_eq!(query_params.underlying_coin_denom, "uluna");
    assert_eq!(query_params.unbonding_period, 210);
    assert_eq!(query_params.peg_recovery_fee, Decimal::zero());
    assert_eq!(query_params.er_threshold, Decimal::one());
    assert_eq!(query_params.reward_denom, "uusd");

    // state storage must be initialized
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    let expected_result = StateResponse {
        bluna_exchange_rate: Decimal::one(),
        stluna_exchange_rate: Decimal::one(),
        total_bond_bluna_amount: Uint128::zero(),
        total_bond_stluna_amount: Uint128::zero(),
        last_index_modification: env.block.time.seconds(),
        prev_hub_balance: Default::default(),
        last_unbonded_time: env.block.time.seconds(),
        last_processed_batch: 0u64,
    };
    assert_eq!(query_state, expected_result);

    // config storage must be initialized
    let conf = Config {};
    let query_conf: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), conf).unwrap()).unwrap();
    let expected_conf = ConfigResponse {
        owner: String::from("owner1"),
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
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(
        query_batch,
        CurrentBatchResponse {
            id: 1,
            requested_bluna_with_fee: Default::default(),
            requested_stluna: Default::default()
        }
    );
}

/// Check that we can not initialize the contract with peg_recovery_fee > 1.0.
#[test]
fn bad_initialization() {
    let mut deps = dependencies(&[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    // successful call
    let msg = InstantiateMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 210,
        peg_recovery_fee: Decimal::from_str("1.1").unwrap(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let owner = String::from("owner1");
    let owner_info = mock_info(owner.as_str(), &[]);

    let res = instantiate(deps.as_mut(), mock_env(), owner_info, msg);
    assert_eq!(
        StdError::generic_err("peg_recovery_fee can not be greater than 1"),
        res.err().unwrap()
    )
}

#[test]
fn proper_bond() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");
    let bond_amount = Uint128::from(10000u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = ExecuteMsg::Bond {};

    let info = mock_info(&addr1, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&String::from("token"), &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[3];
    match mint.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, String::from("token"));
            assert_eq!(
                msg,
                to_binary(&Cw20ExecuteMsg::Mint {
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
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_bluna_amount, bond_amount);
    assert_eq!(query_state.bluna_exchange_rate, Decimal::one());

    // no-send funds
    let bob = String::from("bob");
    let failed_bond = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let bob = String::from("bob");
    let failed_bond = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(10, "ukrt")]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let info = mock_info(
        &addr1,
        &[
            coin(bond_amount.u128(), "uluna"),
            coin(bond_amount.u128(), "uusd"),
        ],
    );

    let res = execute(deps.as_mut(), mock_env(), info, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );
}

#[test]
fn proper_bond_for_st_luna() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");
    let bond_amount = Uint128::from(10000u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&addr1, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[3];
    match mint.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, stluna_token_contract);
            assert_eq!(
                msg,
                to_binary(&Cw20ExecuteMsg::Mint {
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
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_stluna_amount, bond_amount);
    assert_eq!(query_state.stluna_exchange_rate, Decimal::one());

    // no-send funds
    let bob = String::from("bob");
    let failed_bond = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let bob = String::from("bob");
    let failed_bond = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(10, "ukrt")]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let info = mock_info(
        &addr1,
        &[
            coin(bond_amount.u128(), "uluna"),
            coin(bond_amount.u128(), "uusd"),
        ],
    );

    let res = execute(deps.as_mut(), mock_env(), info, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );
}

#[test]
fn proper_bond_rewards() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");
    let bond_amount = Uint128::from(10000u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_dispatcher_contract = String::from("reward_dispatcher");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_dispatcher_contract.clone(),
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator);
    do_register_validator(&mut deps, validator2);
    do_register_validator(&mut deps, validator3);

    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&addr1, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(4, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&addr1, &bond_amount)])]);

    let bond_msg = ExecuteMsg::BondRewards {};

    let info = mock_info(
        &reward_dispatcher_contract,
        &[coin(bond_amount.u128(), "uluna")],
    );

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(3334, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[1];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR2);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let delegate = &res.messages[2];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR3);
            assert_eq!(amount, coin(3333, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    // get total bonded
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(
        query_state.total_bond_stluna_amount,
        bond_amount + bond_amount // BondForStLuna + BondRewards
    );
    assert_eq!(
        query_state.stluna_exchange_rate,
        Decimal::from_ratio(2u128, 1u128)
    );

    // no-send funds
    let failed_bond = ExecuteMsg::BondRewards {};

    let info = mock_info(&reward_dispatcher_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let failed_bond = ExecuteMsg::BondRewards {};

    let info = mock_info(&reward_dispatcher_contract, &[coin(10, "ukrt")]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond.clone());
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //bond with more than one coin is not possible
    let info = mock_info(
        &reward_dispatcher_contract,
        &[
            coin(bond_amount.u128(), "uluna"),
            coin(bond_amount.u128(), "uusd"),
        ],
    );

    let res = execute(deps.as_mut(), mock_env(), info, failed_bond).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("More than one coin is sent; only one asset is supported")
    );

    //bond from non-dispatcher address
    let info = mock_info(
        &String::from("random_address"),
        &[coin(bond_amount.u128(), "uluna")],
    );
    let failed_bond = ExecuteMsg::BondRewards {};

    let res = execute(deps.as_mut(), mock_env(), info, failed_bond).unwrap_err();
    assert_eq!(res, StdError::generic_err("unauthorized"));
}

/// Covers if Withdraw message, swap message, and update global index are sent.
#[test]
pub fn proper_update_global_index() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");
    let bond_amount = Uint128::from(10u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract.clone(),
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    deps.querier
        .with_token_balances(&[(&String::from("token"), &[]), (&stluna_token_contract, &[])]);

    // fails if there is no delegation
    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    // bond
    do_bond(&mut deps, addr1.clone(), bond_amount);
    do_bond_stluna(&mut deps, addr1.clone(), bond_amount);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &bond_amount)]),
    ]);

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] = [(sample_delegation(
        validator.address.clone(),
        coin(bond_amount.u128() * 2, "uluna"),
    ))];

    let validators: [Validator; 1] = [(validator.clone())];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &bond_amount)]),
    ]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let env = mock_env();

    let last_index_query = State {};
    let last_modification: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), last_index_query).unwrap()).unwrap();
    assert_eq!(
        &last_modification.last_index_modification,
        &env.block.time.seconds()
    );

    let withdraw = &res.messages[0];
    match withdraw.msg.clone() {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, validator.address);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let swap = &res.messages[1];
    match swap.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, reward_contract);
            assert_eq!(
                msg,
                to_binary(&SwapToRewardDenom {
                    stluna_total_bonded: Uint128::from(10u64),
                    bluna_total_bonded: Uint128::from(10u64),
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", swap),
    }

    let update_g_index = &res.messages[2];
    match update_g_index.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, reward_contract);
            assert_eq!(msg, to_binary(&DispatchRewards {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", update_g_index),
    }
}

/// Covers update_global_index when there is more than one validator.
/// Checks if more than one Withdraw message is sent.
#[test]
pub fn proper_update_global_index_two_validators() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128::from(10u64));

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &Uint128::from(10u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u64))]),
    ]);

    // register_validator
    do_register_validator(&mut deps, validator2.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128::from(10u64));

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(10, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(10, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator.clone()), (validator2.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &Uint128::from(20u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u64))]),
    ]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(4, res.messages.len());

    let withdraw = &res.messages[0];
    match withdraw.msg.clone() {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, validator.address);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let withdraw = &res.messages[1];
    match withdraw.msg.clone() {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, validator2.address);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }
}

/// Covers update_global_index when more than on validator is registered, but
/// there is only a delegation to only one of them.
/// Checks if one Withdraw message is sent.
#[test]
pub fn proper_update_global_index_respect_one_registered_validator() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128::from(10u64));

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &Uint128::from(10u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u64))]),
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
        (&String::from("token"), &[(&addr1, &Uint128::from(20u128))]),
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u64))]),
    ]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let withdraw = &res.messages[0];
    match withdraw.msg.clone() {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, validator.address);
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
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr0001");
    let invalid = String::from("invalid");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128::from(10u64));
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &Uint128::from(10u128))]),
        (&stluna_token_contract, &[]),
    ]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::from(10u64),
        msg: to_binary(&{}).unwrap(),
    });

    let token_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info, receive);
    assert!(res.is_err());

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::from(10u64),
        msg: to_binary(&failed_unbond).unwrap(),
    });

    let invalid_info = mock_info(&invalid, &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, receive);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // successful call
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128::from(10u64),
        msg: to_binary(&successful_unbond).unwrap(),
    });

    let valid_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), valid_info, receive).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(10u64)
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
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(10u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    //check the current batch before unbond
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    let token_info = mock_info(&token_contract, &[]);

    // check the state before unbond
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(
        query_state.last_unbonded_time,
        mock_env().block.time.seconds()
    );
    assert_eq!(query_state.total_bond_bluna_amount, Uint128::from(10u64));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(1u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), token_info, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(9u128))]),
        (&stluna_token_contract, &[]),
    ]);

    //read the undelegated waitlist of the current epoch for the user bob
    let wait_list = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128::from(1u64), wait_list.bluna_amount);

    //successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(5u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let token_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(4u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(5u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let waitlist2 = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128::from(6u64), waitlist2.bluna_amount);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::from(6u64));

    let mut env = mock_env();
    //pushing time forward to check the unbond message
    env.block.time = env.block.time.plus_seconds(31);

    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(2u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), token_info, receive).unwrap();
    assert_eq!(2, res.messages.len());

    let msg = &res.messages[1];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(2u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    //making sure the sent message (2nd) is undelegate
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    });
    assert_eq!(res.messages[0].msg, msgs);

    // check the current batch
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    // check the state
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, env.block.time.seconds());
    assert_eq!(query_state.total_bond_bluna_amount, Uint128::from(2u64));

    // the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = UnbondRequests { address: bob };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(query_unbond.requests[0].0, 1);
    assert_eq!(query_unbond.requests[0].1, Uint128::from(8u64));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(8u64));
    assert_eq!(res.history[0].bluna_applied_exchange_rate, Decimal::one());
    assert!(
        !res.history[0].released,
        "res.history[0].released is not false"
    );
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the receive message is sent by token contract,
/// if handle_unbond is executed.
/*
    A comprehensive test for unbond is prepared in proper_unbond tests
*/
#[test]
pub fn proper_receive_stluna() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr0001");
    let invalid = String::from("invalid");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond_stluna(&mut deps, addr1.clone(), Uint128::from(10u64));
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u128))]),
        (&String::from("token"), &[]),
    ]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::from(10u64),
        msg: to_binary(&{}).unwrap(),
    });

    let token_info = mock_info(&stluna_token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info, receive);
    assert!(res.is_err());

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::from(10u64),
        msg: to_binary(&failed_unbond).unwrap(),
    });

    let invalid_info = mock_info(&invalid, &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, receive);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // successful call
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128::from(10u64),
        msg: to_binary(&successful_unbond).unwrap(),
    });

    let valid_info = mock_info(&stluna_token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), valid_info, receive).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, stluna_token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(10u64)
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
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(10, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(10u128))]),
        (&token_contract, &[]),
    ]);

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    //check the current batch before unbond
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());
    assert_eq!(query_batch.requested_stluna, Uint128::zero());

    let token_info = mock_info(&stluna_token_contract, &[]);

    // check the state before unbond
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(
        query_state.last_unbonded_time,
        mock_env().block.time.seconds()
    );
    assert_eq!(query_state.total_bond_stluna_amount, Uint128::from(10u64));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(1u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), token_info, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(9u128))]),
        (&token_contract, &[]),
    ]);

    //read the undelegated waitlist of the current epoch for the user bob
    let wait_list = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128::from(1u64), wait_list.stluna_amount);

    //successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(5u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let token_info = mock_info(&stluna_token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(4u128))]),
        (&token_contract, &[]),
    ]);

    let msg = &res.messages[0];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, stluna_token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(5u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let waitlist2 = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128::from(6u64), waitlist2.stluna_amount);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_stluna, Uint128::from(6u64));
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    let mut env = mock_env();
    //pushing time forward to check the unbond message
    env.block.time = env.block.time.plus_seconds(31);

    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::from(2u64),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), env.clone(), token_info, receive).unwrap();
    assert_eq!(2, res.messages.len());

    let msg = &res.messages[1];
    match msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, stluna_token_contract);
            assert_eq!(
                msg,
                to_binary(&Burn {
                    amount: Uint128::from(2u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    //making sure the sent message (2nd) is undelegate
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    });
    assert_eq!(res.messages[0].msg, msgs);

    // check the current batch
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_stluna, Uint128::zero());
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    // check the state
    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, env.block.time.seconds());
    assert_eq!(query_state.total_bond_bluna_amount, Uint128::from(0u64));
    assert_eq!(query_state.total_bond_stluna_amount, Uint128::from(2u64));

    // the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = UnbondRequests { address: bob };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(query_unbond.requests[0].0, 1);
    assert_eq!(query_unbond.requests[0].2, Uint128::from(8u64));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128::from(8u64));
    assert_eq!(res.history[0].stluna_applied_exchange_rate, Decimal::one());
    assert!(
        !res.history[0].released,
        "res.history[0].released is not false"
    );
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the pick_validator function sends different Undelegate messages
/// to different validators, when a validator does not have enough delegation.
#[test]
pub fn proper_pick_validator() {
    let mut deps = dependencies(&[]);

    let addr1 = String::from("addr1000");
    let addr2 = String::from("addr2000");
    let addr3 = String::from("addr3000");

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);

    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3.clone());

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128::from(10u64));
    do_bond(&mut deps, addr2.clone(), Uint128::from(150u64));
    do_bond(&mut deps, addr3.clone(), Uint128::from(200u64));

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
            &String::from("token"),
            &[
                (&addr3, &Uint128::from(200u64)),
                (&addr2, &Uint128::from(150u64)),
                (&addr1, &Uint128::from(10u64)),
            ],
        ),
        (&stluna_token_contract, &[]),
    ]);

    // send the first burn
    let token_info = mock_info(&token_contract, &[]);
    let mut env = mock_env();
    let res = do_unbond(
        deps.as_mut(),
        addr2.clone(),
        env.clone(),
        token_info.clone(),
        Uint128::from(50u64),
    );
    assert_eq!(res.messages.len(), 1);

    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[
                (&addr3, &Uint128::from(200u64)),
                (&addr2, &Uint128::from(100u64)),
                (&addr1, &Uint128::from(10u64)),
            ],
        ),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(40);

    // send the second burn
    let res = do_unbond(
        deps.as_mut(),
        addr2.clone(),
        env,
        token_info,
        Uint128::from(100u64),
    );
    assert_eq!(res.messages.len(), 3);

    deps.querier.with_token_balances(&[(
        &String::from("token"),
        &[
            (&addr3, &Uint128::from(200u64)),
            (&addr2, &Uint128::from(0u64)),
            (&addr1, &Uint128::from(10u64)),
        ],
    )]);

    //check if the undelegate message is send two more than one validator.
    match &res.messages[0].msg.clone() {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(val, &validator3.address);
            assert_eq!(amount.amount, Uint128::from(130u64));
        }
        _ => panic!("Unexpected message: {:?}", &res.messages[0]),
    }
    match &res.messages[1].msg.clone() {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(val, &validator2.address);
            assert_eq!(amount.amount, Uint128::from(20u64));
        }
        _ => panic!("Unexpected message: {:?}", &res.messages[0]),
    }
}

/// Covers if the pick_validator function sends different Undelegate messages
/// if the delegations of the user are distributed to several validators
/// and if the user wants to unbond amount that none of validators has.
#[test]
pub fn proper_pick_validator_respect_distributed_delegation() {
    let mut deps = dependencies(&[]);

    let addr1 = String::from("addr1000");
    let addr2 = String::from("addr2000");

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);

    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3);

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128::from(1000u64));
    do_bond(&mut deps, addr1.clone(), Uint128::from(1500u64));

    // give validators different delegation amount
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(1000, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(1500, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator), (validator2)];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &Uint128::from(2500u64))]),
        (&stluna_token_contract, &[]),
    ]);

    // send the first burn
    let token_info = mock_info(&token_contract, &[]);
    let mut env = mock_env();

    env.block.time = env.block.time.plus_seconds(40);

    let res = do_unbond(
        deps.as_mut(),
        addr2,
        env,
        token_info,
        Uint128::from(2000u64),
    );
    assert_eq!(res.messages.len(), 3);

    match &res.messages[0].msg.clone() {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: _,
            amount,
        }) => assert_eq!(amount.amount, Uint128::from(1250u64)),
        _ => panic!("Unexpected message: {:?}", &res.messages[0]),
    }
}

/// Covers the effect of slashing of bond, unbond, and withdraw_unbonded
/// update the exchange rate after and before slashing.
#[test]
pub fn proper_slashing() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");
    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //bond
    do_bond(&mut deps, addr1.clone(), Uint128::from(1000u64));

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[(&addr1, &Uint128::from(1000u128))],
        ),
        (&stluna_token_contract, &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let info = mock_info(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = execute(deps.as_mut(), mock_env(), info, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    // bonded amount / minted amount
    let expected_er = Decimal::from_ratio(Uint128::from(900u64), Uint128::from(1000u64));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    //bond again to see the update exchange rate
    let second_bond = ExecuteMsg::Bond {};

    let info = mock_info(&addr1, &[coin(1000, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 1900, "uluna");
    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[(&addr1, &Uint128::from(2111u128))],
        ),
        (&stluna_token_contract, &[]),
    ]);

    // expected exchange rate must be greater than 0.9
    let expected_er = Decimal::from_ratio(Uint128::from(1900u64), Uint128::from(2111u64));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(1000, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let message = &res.messages[1];
    match message.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, token_contract);
            assert_eq!(
                msg,
                to_binary(&Mint {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(1111u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    set_delegation(&mut deps.querier, validator.clone(), 1900, "uluna");

    //update user balance
    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[(&addr1, &Uint128::from(2111u128))],
        ),
        (&stluna_token_contract, &[]),
    ]);

    let info = mock_info(&addr1, &[]);
    let mut env = mock_env();
    let _res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        Uint128::from(500u64),
        addr1.clone(),
    )
    .unwrap();

    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[(&addr1, &Uint128::from(1611u128))],
        ),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(31);
    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        Uint128::from(500u64),
        addr1.clone(),
    )
    .unwrap();
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(900, "uluna"),
    });
    assert_eq!(res.messages[0].msg.clone(), msgs);

    deps.querier.with_token_balances(&[
        (
            &String::from("token"),
            &[(&addr1, &Uint128::from(1111u128))],
        ),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);

    let expected_er = Decimal::from_ratio(Uint128::from(1000u128), Uint128::from(1111u128));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    env.block.time = env.block.time.plus_seconds(90);
    //check withdrawUnbonded message
    let withdraw_unbond_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(deps.as_mut(), env, info, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let expected_er = Decimal::from_ratio(Uint128::from(1000u128), Uint128::from(1111u128));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate, expected_er);

    let sent_message = &wdraw_unbonded_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, addr1);
            assert_eq!(amount[0].amount, Uint128::from(900u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }
}

/// Covers the effect of slashing of bond, unbond, and withdraw_unbonded
/// update the exchange rate after and before slashing.
#[test]
pub fn proper_slashing_stluna() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");
    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //bond
    do_bond_stluna(&mut deps, addr1.clone(), Uint128::from(1000u64));

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (
            &stluna_token_contract,
            &[(&addr1, &Uint128::from(1000u128))],
        ),
        (&String::from("token"), &[]),
    ]);

    // slashing
    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let info = mock_info(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = execute(deps.as_mut(), mock_env(), info, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.stluna_exchange_rate.to_string(), "0.9");

    //bond again to see the update exchange rate
    let second_bond = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&addr1, &[coin(900, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    let expected_er = "0.9";
    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(
        query_exchange_rate.stluna_exchange_rate.to_string(),
        expected_er
    );

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(900, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let message = &res.messages[1];
    match message.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, stluna_token_contract);
            assert_eq!(
                msg,
                to_binary(&Mint {
                    recipient: info.sender.to_string(),
                    amount: Uint128::from(1000u64)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    set_delegation(&mut deps.querier, validator.clone(), 1800, "uluna");

    //update user balance
    deps.querier.with_token_balances(&[
        (
            &stluna_token_contract,
            &[(&addr1, &Uint128::from(2000u128))],
        ),
        (&String::from("token"), &[]),
    ]);

    let info = mock_info(&addr1, &[]);
    let mut env = mock_env();
    let _res = execute_unbond_stluna(
        deps.as_mut(),
        env.clone(),
        Uint128::from(500u64),
        addr1.clone(),
    )
    .unwrap();

    deps.querier.with_token_balances(&[
        (
            &stluna_token_contract,
            &[(&addr1, &Uint128::from(1500u128))],
        ),
        (&String::from("token"), &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(31);
    let res = execute_unbond_stluna(
        deps.as_mut(),
        env.clone(),
        Uint128::from(500u64),
        addr1.clone(),
    )
    .unwrap();
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(900, "uluna"),
    });
    assert_eq!(res.messages[0].msg, msgs);

    deps.querier.with_token_balances(&[
        (
            &stluna_token_contract,
            &[(&addr1, &Uint128::from(1000u128))],
        ),
        (&token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(
        query_exchange_rate.stluna_exchange_rate.to_string(),
        expected_er
    );

    env.block.time = env.block.time.plus_seconds(90);
    //check withdrawUnbonded message
    let withdraw_unbond_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(deps.as_mut(), env, info, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(
        query_exchange_rate.stluna_exchange_rate.to_string(),
        expected_er
    );

    let sent_message = &wdraw_unbonded_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, addr1);
            assert_eq!(amount[0].amount, Uint128::from(900u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }
}

/// Covers if the withdraw_rate function is updated before and after withdraw_unbonded,
/// the finished amount is accurate, user requests are removed from the waitlist, and
/// the BankMsg::Send is sent.
#[test]
pub fn proper_withdraw_unbonded() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(100u128))]),
        (&stluna_token_contract, &[]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, 100, "uluna");

    let res = execute_unbond(deps.as_mut(), mock_env(), Uint128::from(10u64), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(90u128))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();
    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);

    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );

    // trigger undelegation message
    assert!(wdraw_unbonded_res.is_err(), "withdraw unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        Uint128::from(10u64),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(80u128))]),
        (&stluna_token_contract, &[]),
    ]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(0u64));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(20u64),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128::from(20u64));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(20u64));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(20u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(20u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128::from(0u64));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
}

/// Covers if the withdraw_rate function is updated before and after withdraw_unbonded,
/// the finished amount is accurate, user requests are removed from the waitlist, and
/// the BankMsg::Send is sent.
#[test]
pub fn proper_withdraw_unbonded_stluna() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(100, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(100u128))]),
        (&String::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let bond_msg = ExecuteMsg::BondRewards {};

    let info = mock_info(&String::from("reward"), &[coin(100, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(1, res.messages.len());

    set_delegation(&mut deps.querier, validator, 200, "uluna");

    let res = execute_unbond_stluna(deps.as_mut(), mock_env(), Uint128::from(10u64), bob.clone())
        .unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(90u128))]),
        (&String::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();
    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);

    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );

    // trigger undelegation message
    assert!(wdraw_unbonded_res.is_err(), "unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    let res = execute_unbond_stluna(
        deps.as_mut(),
        env.clone(),
        Uint128::from(10u64),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(80u128))]),
        (&String::from("token"), &[]),
    ]);

    let state = State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_stluna_amount, Uint128::from(160u64));
    assert_eq!(
        query_state.stluna_exchange_rate,
        Decimal::from_ratio(2u128, 1u128)
    );

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(0u64));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(40u64),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].2, Uint128::from(20u64));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128::from(20u64));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(40u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(40u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128::from(0u64));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
}

#[test]
pub fn proper_withdraw_unbonded_both_tokens() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};
    let bond_for_stluna_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contracts
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(100u128))]),
        (&stluna_token_contract, &[(&bob, &Uint128::from(100u128))]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 100, "uluna");

    let res = execute(deps.as_mut(), mock_env(), info, bond_for_stluna_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 200, "uluna");

    let bond_msg = ExecuteMsg::BondRewards {};

    let info = mock_info(&String::from("reward"), &[coin(100, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(1, res.messages.len());

    set_delegation(&mut deps.querier, validator, 300, "uluna");

    let res = execute_unbond(
        deps.as_mut(),
        mock_env(),
        Uint128::from(100u64),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(0u128))]),
        (&stluna_token_contract, &[(&bob, &Uint128::from(100u128))]),
    ]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();

    env.block.time = env.block.time.plus_seconds(31);
    let res = execute_unbond_stluna(
        deps.as_mut(),
        env.clone(),
        Uint128::from(100u64),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(2, res.messages.len());

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(0u128))]),
        (&stluna_token_contract, &[(&bob, &Uint128::from(0u128))]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(0u64));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(300u64),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128::from(100u64));
    assert_eq!(res.requests[0].2, Uint128::from(100u64));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(100u64));
    assert_eq!(res.history[0].stluna_amount, Uint128::from(100u64));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(300u64));

    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(300u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));

    let waitlist = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(
        query_unbond,
        UnbondRequestsResponse {
            address: bob,
            requests: vec![]
        }
    );

    // because of one that we add for each batch
    let state = State {};
    let state_query: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128::from(0u64));
    assert_eq!(state_query.bluna_exchange_rate, Decimal::one());
    assert_eq!(state_query.stluna_exchange_rate.to_string(), "2");
}

/// Covers slashing during the unbonded period and its effect on the finished amount.
#[test]
pub fn proper_withdraw_unbonded_respect_slashing() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(9500u64))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();

    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );
    assert!(wdraw_unbonded_res.is_err(), "unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = execute_unbond(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&String::from("token"), &[(&bob, &Uint128::from(9000u64))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(0u64));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);

    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128::from(1000u64));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(1000u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(899u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));
}

/// Covers slashing during the unbonded period and its effect on the finished amount.
#[test]
pub fn proper_withdraw_unbonded_respect_slashing_stluna() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&String::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond_stluna(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(9500u64))]),
        (&String::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();

    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );
    assert!(wdraw_unbonded_res.is_err(), "unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res =
        execute_unbond_stluna(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&bob, &Uint128::from(9000u64))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(0u64));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);

    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].2, Uint128::from(1000u64));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(1000u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(899u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));
}

/// Covers withdraw_unbonded/inactivity in the system while there are slashing events.
#[test]
pub fn proper_withdraw_unbonded_respect_inactivity_slashing() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(9500u64))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();
    //set the block time 30 seconds from now.

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, unbond_amount);

    env.block.time = env.block.time.plus_seconds(1000);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );
    assert!(wdraw_unbonded_res.is_err(), "unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = execute_unbond(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&String::from("token"), &[(&bob, &Uint128::from(9000u64))])]);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_bluna_with_fee, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "1");
    assert!(
        !res.history[0].released,
        "res.history[0].released is not false"
    );
    assert_eq!(res.history[0].batch_id, 1);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::zero());

    env.block.time = env.block.time.plus_seconds(1091);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128::from(1000u64));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(1000u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(899u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].bluna_applied_exchange_rate.to_string(), "1");
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "0.899");
    assert!(
        res.history[0].released,
        "res.history[0].released is not true"
    );
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers withdraw_unbonded/inactivity in the system while there are slashing events.
#[test]
pub fn proper_withdraw_unbonded_respect_inactivity_slashing_stluna() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&String::from("token"), &[]),
    ]);

    let delegate = &res.messages[0];
    match delegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond_stluna(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(9500u64))]),
        (&String::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();
    //set the block time 30 seconds from now.

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_stluna, unbond_amount);

    env.block.time = env.block.time.plus_seconds(1000);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        mock_env(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );
    assert!(wdraw_unbonded_res.is_err(), "unbonded error");
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res =
        execute_unbond_stluna(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&stluna_token_contract, &[(&bob, &Uint128::from(9000u64))])]);

    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_stluna, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "1");
    assert!(
        !res.history[0].released,
        "res.history[0].released is not true"
    );
    assert_eq!(res.history[0].batch_id, 1);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::zero());

    env.block.time = env.block.time.plus_seconds(1091);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(900u64),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), mock_env(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].2, Uint128::from(1000u64));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::from(1000u64));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, Uint128::from(899u64))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].stluna_applied_exchange_rate.to_string(), "1");
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "0.899");
    assert!(
        res.history[0].released,
        "res.history[0].released is not true"
    );
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the signed integer works properly,
/// the exception when a user sends rogue coin.
#[test]
pub fn proper_withdraw_unbond_with_dummies() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(
        &mut deps.querier,
        validator.clone(),
        bond_amount.u128(),
        "uluna",
    );

    let res = execute_unbond(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(9500u64))]),
        (&stluna_token_contract, &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();

    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);
    // trigger undelegation message
    let res = execute_unbond(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(9000u64))]),
        (&stluna_token_contract, &[]),
    ]);

    // slashing
    set_delegation(
        &mut deps.querier,
        validator,
        bond_amount.u128() - 2000,
        "uluna",
    );

    let res = execute_unbond(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(8500u64))]),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(31);
    let res = execute_unbond(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &Uint128::from(8000u64))]),
        (&stluna_token_contract, &[]),
    ]);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(2200u64),
        },
    )]);

    env.block.time = env.block.time.plus_seconds(120);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].bluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].bluna_withdraw_rate.to_string(), "1.164");
    assert!(
        res.history[0].released,
        "res.history[0].released is not true"
    );
    assert_eq!(res.history[0].batch_id, 1);
    assert_eq!(res.history[1].bluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[1].bluna_withdraw_rate.to_string(), "1.033");
    assert!(
        res.history[1].released,
        "res.history[1].released is not true"
    );
    assert_eq!(res.history[1].batch_id, 2);

    let expected = (res.history[0].bluna_withdraw_rate * res.history[0].bluna_amount)
        + res.history[1].bluna_withdraw_rate * res.history[1].bluna_amount;
    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, expected)
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));
}

/// Covers if the signed integer works properly,
/// the exception when a user sends rogue coin.
#[test]
pub fn proper_withdraw_unbond_with_dummies_stluna() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::from(10000u64);
    let unbond_amount = Uint128::from(500u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract.clone(),
    );

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::BondForStLuna {};

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &bond_amount)]),
        (&String::from("token"), &[]),
    ]);

    set_delegation(
        &mut deps.querier,
        validator.clone(),
        bond_amount.u128(),
        "uluna",
    );

    let res = execute_unbond_stluna(deps.as_mut(), mock_env(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(9500u64))]),
        (&String::from("token"), &[]),
    ]);

    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(0u64),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();

    //set the block time 30 seconds from now.
    env.block.time = env.block.time.plus_seconds(31);
    // trigger undelegation message
    let res =
        execute_unbond_stluna(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(9000u64))]),
        (&String::from("token"), &[]),
    ]);

    // slashing
    set_delegation(
        &mut deps.querier,
        validator,
        bond_amount.u128() - 2000,
        "uluna",
    );

    let res =
        execute_unbond_stluna(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(8500u64))]),
        (&String::from("token"), &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(31);
    let res =
        execute_unbond_stluna(deps.as_mut(), env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier.with_token_balances(&[
        (&stluna_token_contract, &[(&bob, &Uint128::from(8000u64))]),
        (&String::from("token"), &[]),
    ]);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(2200u64),
        },
    )]);

    env.block.time = env.block.time.plus_seconds(120);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].stluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[0].stluna_withdraw_rate.to_string(), "1.164");
    assert!(
        res.history[0].released,
        "res.history[0].released is not true"
    );
    assert_eq!(res.history[0].batch_id, 1);
    assert_eq!(res.history[1].stluna_amount, Uint128::from(1000u64));
    assert_eq!(res.history[1].stluna_withdraw_rate.to_string(), "1.033");
    assert!(
        res.history[1].released,
        "res.history[1].released is not true"
    );
    assert_eq!(res.history[1].batch_id, 2);

    let expected = (res.history[0].stluna_withdraw_rate * res.history[0].stluna_amount)
        + res.history[1].stluna_withdraw_rate * res.history[1].stluna_amount;
    let sent_message = &success_res.messages[0];
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, bob);
            assert_eq!(amount[0].amount, expected)
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::from(0u64));
}

/// Covers if the state/parameters storage is updated to the given value,
/// who sends the message, and if
/// RewardUpdateDenom message is sent to the reward contract
#[test]
pub fn test_update_params() {
    let mut deps = dependencies(&[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    //test with no swap denom.
    let update_prams = UpdateParams {
        epoch_period: Some(20),
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        paused: Some(false),
    };
    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    let invalid_info = mock_info(String::from("invalid").as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        invalid_info,
        update_prams.clone(),
    );
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));
    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), Params {}).unwrap()).unwrap();
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
        paused: Some(false),
    };

    //the result must be 1
    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_period, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.unbonding_period, 3);
    assert_eq!(params.peg_recovery_fee, Decimal::one());
    assert_eq!(params.er_threshold, Decimal::zero());
    assert_eq!(params.reward_denom, "uusd");

    // Test with peg_recovery_fee > 1.0.
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: Some(Decimal::from_str("1.1").unwrap()),
        er_threshold: None,
        paused: Some(false),
    };

    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams);
    assert_eq!(
        StdError::generic_err("peg_recovery_fee can not be greater than 1"),
        res.err().unwrap()
    );

    //trying to set er_threshold > 1.
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: Some(3),
        peg_recovery_fee: Some(Decimal::one()),
        er_threshold: Some(Decimal::from_str("1.1").unwrap()),
        paused: Some(false),
    };

    //the result must be 1
    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), Params {}).unwrap()).unwrap();

    assert_eq!(params.er_threshold, Decimal::one());
}

/// Covers if peg recovery is applied (in "bond", "unbond",
/// and "withdraw_unbonded" messages) in case of a slashing event
#[test]
pub fn proper_recovery_fee() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: Some(Decimal::from_ratio(
            Uint128::from(1u64),
            Uint128::from(1000u64),
        )),
        er_threshold: Some(Decimal::from_ratio(
            Uint128::from(99u64),
            Uint128::from(100u64),
        )),
        paused: Some(false),
    };
    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    let bond_amount = Uint128::from(1000000u128);
    let unbond_amount = Uint128::from(100000u128);

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let get_params = Params {};
    let parmas: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), get_params).unwrap()).unwrap();
    assert_eq!(parmas.epoch_period, 30);
    assert_eq!(parmas.underlying_coin_denom, "uluna");
    assert_eq!(parmas.unbonding_period, 2);
    assert_eq!(parmas.peg_recovery_fee.to_string(), "0.001");
    assert_eq!(parmas.er_threshold.to_string(), "0.99");

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    //this will set the balance of the user in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 900000, "uluna");

    let report_slashing = CheckSlashing {};
    let res = execute(deps.as_mut(), mock_env(), info, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.bluna_exchange_rate.to_string(), "0.9");

    //Bond again to see the applied result
    let bob = String::from("bob");
    let bond_msg = ExecuteMsg::Bond {};

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &bond_amount)]),
        (&stluna_token_contract, &[]),
    ]);

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);
    let mut env = mock_env();

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    let mint_amount = decimal_division(
        bond_amount,
        Decimal::from_ratio(Uint128::from(9u64), Uint128::from(10u64)),
    );
    let max_peg_fee = mint_amount * parmas.peg_recovery_fee;
    let required_peg_fee =
        (bond_amount + mint_amount + Uint128::zero()) - (Uint128::from(900000u64) + bond_amount);
    let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
    let mint_amount_with_fee = mint_amount - peg_fee;

    let mint_msg = &res.messages[1];
    match mint_msg.msg.clone() {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: _,
            msg,
            funds: _,
        }) => assert_eq!(
            msg,
            to_binary(&Mint {
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
        msg: to_binary(&unbond).unwrap(),
    });

    let new_balance = bond_amount - unbond_amount;

    let token_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());

    //check current batch
    let bonded_with_fee =
        unbond_amount * Decimal::from_ratio(Uint128::from(999u64), Uint128::from(1000u64));
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_bluna_with_fee, bonded_with_fee);

    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &new_balance)]),
        (&stluna_token_contract, &[]),
    ]);

    env.block.time = env.block.time.plus_seconds(60);

    let second_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract,
        amount: unbond_amount,
        msg: to_binary(&second_unbond).unwrap(),
    });

    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), State {}).unwrap()).unwrap();
    let current_batch = CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    let res = execute(deps.as_mut(), env.clone(), token_info.clone(), receive).unwrap();
    assert_eq!(2, res.messages.len());

    let new_balance = new_balance - unbond_amount;
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&bob, &new_balance)]),
        (&stluna_token_contract, &[]),
    ]);

    // during undelegation CurrentBatch.requested_bluna_with_fee sets to zero, so we query requested_bluna_with_fee
    // before test undelegation to calculate unbond_undelegation properly
    let unbond_exchange_rate = Decimal::from_ratio(
        query_state.total_bond_bluna_amount,
        new_balance + query_batch.requested_bluna_with_fee + query_batch.requested_bluna_with_fee,
    );
    let expected = bonded_with_fee + bonded_with_fee;
    let undelegate_message = &res.messages[0];
    match undelegate_message.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(validator.address, val);
            assert_eq!(amount.amount, expected * unbond_exchange_rate);
        }
        _ => panic!("Unexpected message: {:?}", mint_msg),
    }

    //got slashed during unbonding
    deps.querier.with_native_balances(&[(
        String::from(MOCK_CONTRACT_ADDR),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::from(161870u64),
        },
    )]);

    env.block.time = env.block.time.plus_seconds(90);
    //check withdrawUnbonded message
    let withdraw_unbond_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(deps.as_mut(), env, token_info, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let sent_message = &wdraw_unbonded_res.messages[0];
    let expected = (expected
        * unbond_exchange_rate
        * Decimal::from_ratio(Uint128::from(161870u64), expected * unbond_exchange_rate))
        - Uint128::from(1u64);
    match sent_message.msg.clone() {
        CosmosMsg::Bank(BankMsg::Send {
            to_address: _,
            amount,
        }) => {
            assert_eq!(amount[0].amount, expected);
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    // amount should be 99 + 99 since we store the requested amount with peg fee applied.
    assert_eq!(
        res.history[0].bluna_amount,
        bonded_with_fee + bonded_with_fee
    );
    assert_eq!(
        res.history[0].bluna_applied_exchange_rate,
        unbond_exchange_rate
    );
    assert_eq!(
        res.history[0].bluna_withdraw_rate,
        Decimal::from_ratio(Uint128::from(161869u64), bonded_with_fee + bonded_with_fee)
    );
    assert!(res.history[0].released, "history[0].released is not true");
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the storage affected by update_config are updated properly
#[test]
pub fn proper_update_config() {
    let mut deps = dependencies(&[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let new_owner = String::from("new_owner");
    let invalid_owner = String::from("invalid_owner");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");
    let airdrop_registry = String::from("airdrop_registry");

    initialize(
        deps.borrow_mut(),
        owner.clone(),
        reward_contract.clone(),
        token_contract.clone(),
        stluna_token_contract.clone(),
    );

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
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
    let info = mock_info(&invalid_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // change the owner
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = CONFIG.load(&deps.storage).unwrap();
    let new_owner_raw = deps.api.addr_canonicalize(&new_owner).unwrap();
    assert_eq!(new_owner_raw, config.creator);

    // new owner can send the owner related messages
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        paused: Some(false),
    };

    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    //previous owner cannot send this message
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        paused: Some(false),
    };

    let new_owner_info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_prams);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: Some(String::from("new reward")),
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg: CosmosMsg = CosmosMsg::Distribution(DistributionMsg::SetWithdrawAddress {
        address: String::from("new reward"),
    });
    assert_eq!(msg, res.messages[0].msg.clone());

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.reward_dispatcher_contract.unwrap(),
        String::from("new reward")
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        bluna_token_contract: Some(String::from("new token")),
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("updating bLuna token address is forbidden",)
    );

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();

    //make sure the other configs are still the same.
    assert_eq!(
        config_query.reward_dispatcher_contract.unwrap(),
        String::from("new reward")
    );
    assert_eq!(config_query.owner, new_owner);
    assert_eq!(
        config_query.bluna_token_contract.unwrap(),
        String::from("token")
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: Some(String::from("new airdrop")),
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.airdrop_registry_contract.unwrap(),
        String::from("new airdrop")
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: Some(String::from("new registry")),
        bluna_token_contract: None,
        stluna_token_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.validators_registry_contract.unwrap(),
        String::from("new registry"),
    );

    let update_config = UpdateConfig {
        owner: None,
        rewards_dispatcher_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        bluna_token_contract: None,
        stluna_token_contract: Some(stluna_token_contract.clone()),
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("updating stLuna token address is forbidden",)
    );

    let config = Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.stluna_token_contract.unwrap(),
        stluna_token_contract,
    );
}

#[test]
fn proper_claim_airdrop() {
    let mut deps = dependencies(&[]);

    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");
    let airdrop_registry = String::from("airdrop_registry");

    initialize(
        deps.borrow_mut(),
        owner.clone(),
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    let claim_msg = ExecuteMsg::ClaimAirdrop {
        airdrop_token_contract: String::from("airdrop_token"),
        airdrop_contract: String::from("MIR_contract"),
        airdrop_swap_contract: String::from("airdrop_swap"),
        claim_msg: to_binary(&MIRMsg::MIRClaim {}).unwrap(),
        swap_msg: Default::default(),
    };

    //invalid sender
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, claim_msg.clone()).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err(format!("Sender must be {}", &airdrop_registry))
    );

    let valid_info = mock_info(&airdrop_registry, &[]);
    let res = execute(deps.as_mut(), mock_env(), valid_info, claim_msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    assert_eq!(
        res.messages[0].msg.clone(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("MIR_contract"),
            msg: to_binary(&MIRMsg::MIRClaim {}).unwrap(),
            funds: vec![],
        })
    );
    assert_eq!(
        res.messages[1].msg.clone(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: mock_env().contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SwapHook {
                airdrop_token_contract: String::from("airdrop_token"),
                airdrop_swap_contract: String::from("airdrop_swap"),
                swap_msg: Default::default(),
            })
            .unwrap(),
            funds: vec![],
        })
    );
}

#[test]
fn proper_swap_hook() {
    let mut deps = dependencies(&[]);

    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner.clone(),
        reward_contract.clone(),
        token_contract,
        stluna_token_contract,
    );

    let swap_msg = ExecuteMsg::SwapHook {
        airdrop_token_contract: String::from("airdrop_token"),
        airdrop_swap_contract: String::from("swap_contract"),
        swap_msg: to_binary(&PairHandleMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: Some(reward_contract.clone()),
        })
        .unwrap(),
    };

    //invalid sender
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, swap_msg.clone()).unwrap_err();
    assert_eq!(res, StdError::generic_err("unauthorized"));

    // no balance for hub
    let contract_info = mock_info(&mock_env().contract.address.to_string(), &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        contract_info.clone(),
        swap_msg.clone(),
    );
    assert!(res.is_err());

    deps.querier.with_token_balances(&[(
        &String::from("airdrop_token"),
        &[(
            &mock_env().contract.address.to_string(),
            &Uint128::from(1000u64),
        )],
    )]);

    let res = execute(deps.as_mut(), mock_env(), contract_info, swap_msg).unwrap();
    assert_eq!(res.messages.len(), 1);
    assert_eq!(
        res.messages[0].msg.clone(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("airdrop_token"),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: String::from("swap_contract"),
                amount: Uint128::from(1000u64),
                msg: to_binary(&PairHandleMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: Some(reward_contract),
                })
                .unwrap(),
            })
            .unwrap(),
            funds: vec![],
        })
    )
}

#[test]
fn proper_update_global_index_with_airdrop() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");
    let bond_amount = Uint128::from(10u64);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
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
        [(sample_delegation(validator.address.clone(), coin(bond_amount.u128(), "uluna")))];

    let validators: [Validator; 1] = [(validator)];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[
        (&String::from("token"), &[(&addr1, &bond_amount)]),
        (&stluna_token_contract, &[(&addr1, &Uint128::from(10u64))]),
    ]);

    let binary_msg = to_binary(&FabricateMIRClaim {
        stage: 0,
        amount: Uint128::from(1000u64),
        proof: vec!["proof".to_string()],
    })
    .unwrap();

    let binary_msg2 = to_binary(&FabricateANCClaim {
        stage: 0,
        amount: Uint128::from(1000u64),
        proof: vec!["proof".to_string()],
    })
    .unwrap();
    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: Some(vec![binary_msg.clone(), binary_msg2.clone()]),
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(5, res.messages.len());

    assert_eq!(
        res.messages[0].msg.clone(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("airdrop_registry"),
            msg: binary_msg,
            funds: vec![],
        })
    );

    assert_eq!(
        res.messages[1].msg.clone(),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: String::from("airdrop_registry"),
            msg: binary_msg2,
            funds: vec![],
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

fn sample_delegation(addr: String, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = coins(0, &amount.denom);
    FullDelegation {
        validator: addr,
        delegator: Addr::unchecked(String::from(MOCK_CONTRACT_ADDR)),
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}

// sample MIR claim msg
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::upper_case_acronyms)]
pub enum MIRMsg {
    MIRClaim {},
}

#[test]
fn test_convert_to_stluna_with_slashing_and_peg_fee() {
    let mut deps = dependencies(&coins(2, "token"));
    let sender_addr = String::from("addr001");
    let owner = String::from("owner1");
    let bluna_token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        bluna_token_contract.clone(),
        stluna_token_contract.clone(),
    );
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);
    do_register_validator(&mut deps, validator.clone());

    STATE
        .update(&mut deps.storage, |mut prev_state| -> StdResult<_> {
            prev_state.total_bond_stluna_amount = Uint128::from(2000u64);
            prev_state.total_bond_bluna_amount = Uint128::from(2000u64);
            // set stluna er to 1.2, slashing during convert procedure should make it equal to 1
            prev_state.stluna_exchange_rate =
                Decimal::from_ratio(Uint128::from(12u64), Uint128::from(10u64));
            Ok(prev_state)
        })
        .unwrap();
    // delegating only 3200 = simulate we have delegated 4000 and we lost 800 on slashing
    // slashing shoud lead us to
    // bluna er = (2000-400)/2000 = 0.8
    // stluna er = (2000-400)/1600 = 1
    set_delegation(&mut deps.querier, validator, 3200, "uluna");
    PARAMETERS
        .update(&mut deps.storage, |mut prev_param| -> StdResult<_> {
            prev_param.peg_recovery_fee = Decimal::from_str("0.05")?;
            Ok(prev_param)
        })
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &String::from("stluna_token"),
            &[(&sender_addr, &Uint128::from(1600u64))],
        ),
        (
            &String::from("token"),
            &[(&sender_addr, &Uint128::from(2000u64))],
        ),
    ]);
    /*

    bluna_amount = 1000
    max_peg_fee = bluna_amount * recovery_fee = 1000 * 0.05 = 50
    required_peg_fee = 1000(bluna supply) - 800 = 200
    peg_fee = min(50,200) = 50
    bluna_amount_with_fee = 1000-50 = 950

    we burn 1000bluna, but converting only 950, denom eqiuv 950*0.8 = 760
    we mint 760stluna tokens

    let bluna_amount_with_fee: Uint128;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_amount * recovery_fee;
        let required_peg_fee = (total_bluna_supply + current_batch.requested_bluna_with_fee)
            .checked_sub(state.total_bond_bluna_amount)?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_amount_with_fee = bluna_amount.checked_sub(peg_fee)?;
    } else {
        bluna_amount_with_fee = bluna_amount;
    }

    new bluna exchange rate
    bonded amount bluna before convert - 1600
    convert denom equiv(950 bluna) - 760
    new bonded amount - 840
    new bluna tokens amount - 1000
    new rate = 840/1000 - 0.84
    */

    let info = mock_info(bluna_token_contract.as_str(), &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: sender_addr.clone(),
        amount: Uint128::from(1000u64),
        msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
    });
    let r = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    let applied_exchange_rate = &r
        .attributes
        .iter()
        .find(|a| a.key == "bluna_exchange_rate")
        .unwrap()
        .value;
    assert_eq!("0.8", applied_exchange_rate);

    let bluna_minted_with_fee = &r
        .attributes
        .iter()
        .find(|a| a.key == "bluna_amount")
        .unwrap()
        .value;
    assert_eq!("1000", bluna_minted_with_fee);

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender_addr,
        amount: Uint128::from(760u128),
    };
    assert_eq!(
        r.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: stluna_token_contract,
            msg: to_binary(&mint_msg).unwrap(),
            funds: vec![],
        })
    );

    let burn_msg = Cw20ExecuteMsg::Burn {
        amount: Uint128::from(1000u128),
    };
    assert_eq!(
        r.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: bluna_token_contract,
            msg: to_binary(&burn_msg).unwrap(),
            funds: vec![],
        })
    );

    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), State {}).unwrap()).unwrap();
    let new_exchange = query_exchange_rate.bluna_exchange_rate;
    assert_eq!("0.42", new_exchange.to_string());
}

#[test]
fn test_convert_to_bluna_with_slashing_and_peg_fee() {
    let mut deps = dependencies(&coins(2, "token"));
    let sender_addr = String::from("addr001");
    let owner = String::from("owner1");
    let bluna_token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        bluna_token_contract.clone(),
        stluna_token_contract.clone(),
    );
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);
    do_register_validator(&mut deps, validator.clone());

    STATE
        .update(&mut deps.storage, |mut prev_state| -> StdResult<_> {
            prev_state.total_bond_stluna_amount = Uint128::from(2000u64);
            prev_state.total_bond_bluna_amount = Uint128::from(2000u64);
            // set stluna er to 1.2, slashing during convert procedure should make it equal to 1
            prev_state.stluna_exchange_rate =
                Decimal::from_ratio(Uint128::from(12u64), Uint128::from(10u64));
            Ok(prev_state)
        })
        .unwrap();
    // delegating only 3200 = simulate we have delegated 4000 and we lost 800 on slashing
    // slashing shoud lead us to
    // bluna er = (2000-400)/2000 = 0.8
    // stluna er = (2000-400)/1600 = 1
    set_delegation(&mut deps.querier, validator, 3200, "uluna");
    PARAMETERS
        .update(&mut deps.storage, |mut prev_param| -> StdResult<_> {
            prev_param.peg_recovery_fee = Decimal::from_str("0.05")?;
            Ok(prev_param)
        })
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &String::from("stluna_token"),
            &[(&sender_addr, &Uint128::from(1600u64))],
        ),
        (
            &String::from("token"),
            &[(&sender_addr, &Uint128::from(2000u64))],
        ),
    ]);
    /*
    we burn 1000stluna tokens for 1000uluna (denom_equiv)
    bluna_exchange_rate = 0.8
    bluna_to_mint = 1000/0.8 = 1250
    max_peg_fee = bluna_to_mint * recovery_fee = 1250 * 0.05 = 62
    required_peg_fee = 2000 + 1250 - (1600+1000) = 650
    peg_fee = min(62,650) = 62
    bluna_mint_amount_with_fee = bluna_to_mint - 62 = 1188

    bluna_exchange_rate after conversion = (1600(bluna bonded amount before) + 1000(denom_equiv)) / (2000 + 1188) = 0.81555834....

    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_to_mint * recovery_fee;
        let required_peg_fee = (total_bluna_supply + bluna_to_mint + requested_bluna_with_fee)
            - (state.total_bond_bluna_amount + denom_equiv);
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_mint_amount_with_fee = bluna_to_mint.checked_sub(peg_fee)?;
    }
    */

    let info = mock_info(stluna_token_contract.as_str(), &[]);
    let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
        sender: sender_addr.clone(),
        amount: Uint128::from(1000u64),
        msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
    });
    let r = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    let applied_exchange_rate = &r
        .attributes
        .iter()
        .find(|a| a.key == "bluna_exchange_rate")
        .unwrap()
        .value;
    assert_eq!("0.8", applied_exchange_rate);

    let bluna_minted_with_fee = &r
        .attributes
        .iter()
        .find(|a| a.key == "bluna_amount")
        .unwrap()
        .value;
    assert_eq!("1188", bluna_minted_with_fee);

    let mint_msg = Cw20ExecuteMsg::Mint {
        recipient: sender_addr,
        amount: Uint128::from(1188u128),
    };
    assert_eq!(
        r.messages[0].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: bluna_token_contract,
            msg: to_binary(&mint_msg).unwrap(),
            funds: vec![],
        })
    );

    let burn_msg = Cw20ExecuteMsg::Burn {
        amount: Uint128::from(1000u128),
    };
    assert_eq!(
        r.messages[1].msg,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: stluna_token_contract,
            msg: to_binary(&burn_msg).unwrap(),
            funds: vec![],
        })
    );

    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), State {}).unwrap()).unwrap();
    let new_exchange = query_exchange_rate.bluna_exchange_rate;
    assert_eq!("1.3", new_exchange.to_string());
}

#[test]
fn test_receive_cw20() {
    let mut deps = dependencies(&coins(2, "token"));
    let sender_addr = String::from("addr001");
    let owner = String::from("owner1");
    let bluna_token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner,
        reward_contract,
        bluna_token_contract.clone(),
        stluna_token_contract.clone(),
    );
    STATE
        .update(&mut deps.storage, |mut prev_state| -> StdResult<_> {
            prev_state.total_bond_stluna_amount = Uint128::from(1000u64);
            prev_state.total_bond_bluna_amount = Uint128::from(1000u64);
            Ok(prev_state)
        })
        .unwrap();
    deps.querier.with_token_balances(&[
        (
            &String::from("stluna_token"),
            &[(&sender_addr, &Uint128::from(1000u64))],
        ),
        (
            &String::from("token"),
            &[(&sender_addr, &Uint128::from(1000u64))],
        ),
    ]);
    {
        // just enough stluna tokens to convert
        let info = mock_info(stluna_token_contract.as_str(), &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128::from(1000u64),
            msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
        });
        let _ = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }
    {
        // does not enough stluna tokens to convert
        let info = mock_info(stluna_token_contract.as_str(), &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128::from(1001u64),
            msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            StdError::generic_err(
                "Decrease amount cannot exceed total stluna bond amount: 0. Trying to reduce: 1001"
            ),
            err
        );
    }

    {
        // just enough bluna tokens to convert
        let info = mock_info(bluna_token_contract.as_str(), &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr.clone(),
            amount: Uint128::from(1000u64),
            msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
        });
        let _ = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    }
    {
        // does not enough bluna tokens to convert
        let info = mock_info(bluna_token_contract.as_str(), &[]);
        let msg = ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender: sender_addr,
            amount: Uint128::from(1001u64),
            msg: to_binary(&Cw20HookMsg::Convert {}).unwrap(),
        });
        let err = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
        assert_eq!(
            StdError::generic_err(
                "Decrease amount cannot exceed total bluna bond amount: 1000. Trying to reduce: 1001"
            ),
            err
        );
    }
}

#[test]
fn proper_redelegate_proxy() {
    let mut deps = dependencies(&[]);

    set_validator_mock(&mut deps.querier);

    let addr1 = String::from("addr1000");

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");
    let validators_registry = String::from("validators_registry");

    initialize(
        deps.borrow_mut(),
        owner.clone(),
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    let redelegate_proxy_msg = ExecuteMsg::RedelegateProxy {
        src_validator: String::from("src_validator"),
        redelegations: vec![(String::from("dst_validator"), Coin::new(100, "uluna"))],
    };

    //invalid sender
    let info = mock_info(&addr1, &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        info,
        redelegate_proxy_msg.clone(),
    )
    .unwrap_err();
    assert_eq!(res, StdError::generic_err("unauthorized"));

    // check that validators_registry can send such messages
    let info = mock_info(&validators_registry, &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        info,
        redelegate_proxy_msg.clone(),
    )
    .unwrap();

    let redelegate = &res.messages[0];
    match redelegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Redelegate {
            src_validator,
            dst_validator,
            amount,
        }) => {
            assert_eq!(src_validator, String::from("src_validator"));
            assert_eq!(dst_validator, String::from("dst_validator"));
            assert_eq!(amount, Coin::new(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", redelegate),
    }

    // check that creator can send such messages
    let info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, redelegate_proxy_msg).unwrap();

    let redelegate = &res.messages[0];
    match redelegate.msg.clone() {
        CosmosMsg::Staking(StakingMsg::Redelegate {
            src_validator,
            dst_validator,
            amount,
        }) => {
            assert_eq!(src_validator, String::from("src_validator"));
            assert_eq!(dst_validator, String::from("dst_validator"));
            assert_eq!(amount, Coin::new(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", redelegate),
    }
}

///
#[test]
pub fn test_pause() {
    let mut deps = dependencies(&[]);

    let _validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = String::from("owner1");
    let token_contract = String::from("token");
    let stluna_token_contract = String::from("stluna_token");
    let reward_contract = String::from("reward");

    initialize(
        deps.borrow_mut(),
        owner.clone(),
        reward_contract,
        token_contract,
        stluna_token_contract,
    );

    // set paused = true
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: Some(3),
        peg_recovery_fee: Some(Decimal::one()),
        er_threshold: Some(Decimal::zero()),
        paused: Some(true),
    };
    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();

    // try to run a not allowed action (anything but update config and migrate_unbond_wait_list),
    // should return an error
    let update_config = UpdateConfig {
        owner: Some(owner.clone()),
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let info = mock_info(&owner.clone(), &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("the contact is temporarily paused")
    );

    // un-pause the contract
    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: Some(3),
        peg_recovery_fee: Some(Decimal::one()),
        er_threshold: Some(Decimal::zero()),
        paused: Some(false),
    };
    let creator_info = mock_info(String::from("owner1").as_str(), &[]);
    execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();

    // execute the same handler, should work
    let update_config = UpdateConfig {
        owner: Some(owner.clone()),
        rewards_dispatcher_contract: None,
        bluna_token_contract: None,
        airdrop_registry_contract: None,
        validators_registry_contract: None,
        stluna_token_contract: None,
    };
    let info = mock_info(&owner.clone(), &[]);
    execute(deps.as_mut(), mock_env(), info, update_config).unwrap();
}
