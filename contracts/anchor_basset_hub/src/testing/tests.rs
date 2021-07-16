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
//! 4. Anywhere you see query(deps.as_ref(), ...) you must replace it with query(&mut deps, ...)
use cosmwasm_std::{
    coin, from_binary, to_binary, Addr, Api, BankMsg, Coin, CosmosMsg, Decimal, DepsMut,
    DistributionMsg, Env, FullDelegation, MessageInfo, OwnedDeps, Querier, Response, StakingMsg,
    StdError, Storage, SubMsg, Uint128, Validator, WasmMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::testing::{mock_env, mock_info};

use crate::contract::{execute, instantiate, query};
use crate::unbond::execute_unbond;
use basset::hub::QueryMsg;
use basset::hub::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, ExecuteMsg, InstantiateMsg,
    StateResponse, UnbondRequestsResponse, WhitelistedValidatorsResponse,
    WithdrawableUnbondedResponse,
};

use basset::hub::Cw20HookMsg::Unbond;
use basset::hub::ExecuteMsg::{CheckSlashing, Receive, UpdateConfig, UpdateParams};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw20_base::msg::ExecuteMsg::{Burn, Mint};

use super::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
use crate::math::decimal_division;
use crate::state::{read_unbond_wait_list, Parameters, CONFIG};
use basset::airdrop::ExecuteMsg::{FabricateANCClaim, FabricateMIRClaim};
use basset::airdrop::PairHandleMsg;
use basset::hub::QueryMsg::{AllHistory, UnbondRequests, WithdrawableUnbonded};
use basset::reward::ExecuteMsg::{SwapToRewardDenom, UpdateGlobalIndex};
use std::borrow::BorrowMut;

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2000";
const DEFAULT_VALIDATOR3: &str = "default-validator3000";

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub const INITIAL_DEPOSIT_AMOUNT: Uint128 = Uint128::new(1000000u128);

fn sample_validator(addr: String) -> Validator {
    Validator {
        address: addr,
        commission: Decimal::percent(3),
        max_commission: Decimal::percent(10),
        max_change_rate: Decimal::percent(1),
    }
}

fn set_validator_mock(querier: &mut WasmMockQuerier) {
    querier.update_staking(
        "uluna",
        &[
            sample_validator(DEFAULT_VALIDATOR.to_string()),
            sample_validator(DEFAULT_VALIDATOR2.to_string()),
            sample_validator(DEFAULT_VALIDATOR3.to_string()),
        ],
        &[],
    );
}

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut OwnedDeps<S, A, Q>,
    owner: String,
    reward_contract: String,
    token_contract: String,
    validator: String,
) {
    let msg = InstantiateMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
        validator,
    };

    let owner_info = mock_info(owner.as_str(), &[coin(1000000, "uluna")]);
    instantiate(deps.as_mut(), mock_env(), owner_info.clone(), msg).unwrap();

    let register_msg = ExecuteMsg::UpdateConfig {
        owner: None,
        reward_contract: Some(reward_contract),
        token_contract: Some(token_contract),
        airdrop_registry_contract: Some("airdrop_registry".to_string()),
    };

    let res = execute(deps.as_mut(), mock_env(), owner_info, register_msg).unwrap();
    assert_eq!(1, res.messages.len());
}

pub fn do_register_validator(deps: DepsMut, validator: Validator) {
    let owner = "owner1";

    let owner_info = mock_info(owner, &[]);
    let msg = ExecuteMsg::RegisterValidator {
        validator: validator.address,
    };

    let res = execute(deps, mock_env(), owner_info, msg).unwrap();
    assert_eq!(0, res.messages.len());
}

pub fn do_bond(deps: DepsMut, addr: String, amount: Uint128, validator: Validator) {
    let bond = ExecuteMsg::Bond {
        validator: validator.address,
    };

    let info = mock_info(&addr, &[coin(amount.u128(), "uluna")]);
    let res = execute(deps, mock_env(), info, bond).unwrap();
    assert_eq!(2, res.messages.len());
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

/// Covers if all the fields of InitMsg are stored in
/// parameters' storage, the config storage stores the creator,
/// the current batch storage and state are initialized.
#[test]
fn proper_initialization() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    // successful call
    let msg = InstantiateMsg {
        epoch_period: 30,
        underlying_coin_denom: "uluna".to_string(),
        unbonding_period: 210,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
        validator: validator.address.clone(),
    };

    let _owner = "owner1";
    let owner_info = mock_info("owner1", &[coin(1000000, "uluna")]);

    // we can just call .unwrap() to assert this was a success
    let res: Response = instantiate(deps.as_mut(), mock_env(), owner_info.clone(), msg).unwrap();
    assert_eq!(2, res.messages.len());

    let register_validator = ExecuteMsg::RegisterValidator {
        validator: validator.address.clone(),
    };
    let reg_validator_msg = SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: mock_env().contract.address.to_string(),
        msg: to_binary(&register_validator).unwrap(),
        funds: vec![],
    }));

    assert_eq!(&res.messages[0], &reg_validator_msg);

    let delegate_msg = SubMsg::new(CosmosMsg::Staking(StakingMsg::Delegate {
        validator: validator.address,
        amount: coin(1000000, "uluna"),
    }));

    assert_eq!(&res.messages[1], &delegate_msg);

    // check parameters storage
    let params = QueryMsg::Parameters {};
    let query_params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), params).unwrap()).unwrap();
    assert_eq!(query_params.epoch_period, 30);
    assert_eq!(query_params.underlying_coin_denom, "uluna");
    assert_eq!(query_params.unbonding_period, 210);
    assert_eq!(query_params.peg_recovery_fee, Decimal::zero());
    assert_eq!(query_params.er_threshold, Decimal::one());
    assert_eq!(query_params.reward_denom, "uusd");

    // state storage must be initialized
    let state = QueryMsg::State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    let expected_result = StateResponse {
        exchange_rate: Decimal::one(),
        total_bond_amount: owner_info.funds[0].amount,
        last_index_modification: mock_env().block.time.nanos(),
        prev_hub_balance: Default::default(),
        actual_unbonded_amount: Default::default(),
        last_unbonded_time: mock_env().block.time.nanos(),
        last_processed_batch: 0u64,
    };
    assert_eq!(query_state, expected_result);

    // config storage must be initialized
    let conf = QueryMsg::Config {};
    let query_conf: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), conf).unwrap()).unwrap();
    let expected_conf = ConfigResponse {
        owner: "owner1".to_string(),
        reward_contract: None,
        token_contract: None,
        airdrop_registry_contract: None,
    };

    assert_eq!(expected_conf, query_conf);

    // current branch storage must be initialized
    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(
        query_batch,
        CurrentBatchResponse {
            id: 1,
            requested_with_fee: Default::default()
        }
    );
}

/// Covers if a given validator is registered in whitelisted validator storage.
#[test]
fn proper_register_validator() {
    let mut deps = dependencies(&[]);

    // first need to have validators
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // send by invalid user

    let owner_info = mock_info("invalid", &[]);
    let msg = ExecuteMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    // invalid requests
    let res = execute(deps.as_mut(), mock_env(), owner_info, msg);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    //invalid validator

    let owner_info = mock_info("owner1", &[]);
    let msg = ExecuteMsg::RegisterValidator {
        validator: "The specified address is not a validator".to_string(),
    };

    let res = execute(deps.as_mut(), mock_env(), owner_info, msg);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Invalid input: human address too long")
    );

    // successful call

    let owner_info = mock_info("owner1", &[]);
    let msg = ExecuteMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = execute(deps.as_mut(), mock_env(), owner_info.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let query_validatator = QueryMsg::WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_validatator).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(0).unwrap(), &validator.address);

    // register another validator
    let msg = ExecuteMsg::RegisterValidator {
        validator: validator2.address.clone(),
    };

    let res = execute(deps.as_mut(), mock_env(), owner_info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // check if the validator is sored;
    let query_validatator2 = QueryMsg::WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_validatator2).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(1).unwrap(), &validator2.address);
    assert_eq!(query_res.validators.get(0).unwrap(), &validator.address);
}

/// Covers if delegate message is sent to the specified validator,
/// mint message is sent to the token contract, state is changed based on new mint,
/// and check unsuccessful calls, like unsupported validators, and invalid coin.
#[test]
fn proper_bond() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();
    let bond_amount = Uint128::new(10000);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    let _info = mock_info(addr1.as_str(), &[]);
    // set balance for hub contract
    set_delegation(
        &mut deps.querier,
        validator.clone(),
        INITIAL_DEPOSIT_AMOUNT.u128(),
        "uluna",
    );

    deps.querier.with_token_balances(&[(
        &"token".to_string(),
        &[(
            &mock_env().contract.address.to_string(),
            &INITIAL_DEPOSIT_AMOUNT,
        )],
    )]);

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address,
    };

    let info = mock_info(addr1.as_str(), &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[1].msg;
    match mint {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &"token".to_string());
            assert_eq!(
                msg,
                &to_binary(&Cw20ExecuteMsg::Mint {
                    recipient: addr1.clone(),
                    amount: bond_amount
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", mint),
    }

    // get total bonded
    let state = QueryMsg::State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(
        query_state.total_bond_amount,
        bond_amount + INITIAL_DEPOSIT_AMOUNT
    );
    assert_eq!(query_state.exchange_rate, Decimal::one());

    //test unsupported validator
    let invalid_validator = "invalid";
    let bob = "bob".to_string();
    let bond = ExecuteMsg::Bond {
        validator: invalid_validator.to_string(),
    };

    let info = mock_info(&bob, &[coin(10, "uluna")]);
    let res = execute(deps.as_mut(), mock_env(), info, bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("The chosen validator is currently not supported")
    );

    // no-send funds
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let bob = "bob".to_string();
    let failed_bond = ExecuteMsg::Bond {
        validator: validator.address,
    };

    let info = mock_info(&bob, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna assets are provided to bond")
    );

    //send other tokens than luna funds
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let bob = "bob".to_string();
    let failed_bond = ExecuteMsg::Bond {
        validator: validator.address,
    };

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

/// Covers if the Redelegate message and UpdateGlobalIndex are sent.
/// It also checks if the validator is removed from the storage.
#[test]
fn proper_deregister() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    set_validator_mock(&mut deps.querier);

    let delegated_amount = Uint128::new(10);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner.clone(),
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // register_validator2
    do_register_validator(deps.as_mut(), validator2.clone());

    //must be able to deregister while there is no delegation
    let msg = ExecuteMsg::DeregisterValidator {
        validator: validator.address.clone(),
    };

    let owner_info = mock_info(owner.as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), owner_info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // register_validator 1 again
    do_register_validator(deps.as_mut(), validator.clone());

    set_delegation(
        &mut deps.querier,
        validator.clone(),
        delegated_amount.u128(),
        "uluna",
    );

    // check invalid sender
    let msg = ExecuteMsg::DeregisterValidator {
        validator: validator.address.clone(),
    };

    let invalid_info = mock_info("invalid", &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, msg);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let msg = ExecuteMsg::DeregisterValidator {
        validator: validator.address.clone(),
    };

    let owner_info = mock_info(owner.as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), owner_info, msg).unwrap();
    assert_eq!(2, res.messages.len());

    let redelegate_msg = &res.messages[0].msg;
    match redelegate_msg {
        CosmosMsg::Staking(StakingMsg::Redelegate {
            src_validator,
            dst_validator,
            amount,
        }) => {
            assert_eq!(src_validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(dst_validator.as_str(), DEFAULT_VALIDATOR2.to_string());
            assert_eq!(amount, &coin(delegated_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", redelegate_msg),
    }

    let global_index = &res.messages[1].msg;
    match global_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, MOCK_CONTRACT_ADDR);
            assert_eq!(
                msg,
                &to_binary(&ExecuteMsg::UpdateGlobalIndex {
                    airdrop_hooks: None
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", redelegate_msg),
    }

    let query_validator = QueryMsg::WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_validator).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(0).unwrap(), &validator2.address);
    assert!(!query_res.validators.contains(&validator.address));

    // fails if there is only one validator
    let msg = ExecuteMsg::DeregisterValidator {
        validator: validator2.address,
    };

    let owner_info = mock_info(owner.as_str(), &[]);
    let res = execute(deps.as_mut(), mock_env(), owner_info, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("Cannot remove the last whitelisted validator")
    );
}

/// Covers if Withdraw message, swap message, and update global index are sent.
#[test]
pub fn proper_update_global_index() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();
    let bond_amount = Uint128::new(10);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract.clone(),
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // fails if there is no delegation
    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(res.messages.len(), 2);

    // bond
    do_bond(deps.as_mut(), addr1.clone(), bond_amount, validator.clone());

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(bond_amount.u128(), "uluna")))];

    let validators: [Validator; 1] = [(validator.clone())];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &bond_amount)])]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let last_index_query = QueryMsg::State {};
    let last_modification: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), last_index_query).unwrap()).unwrap();
    assert_eq!(
        &last_modification.last_index_modification,
        &mock_env().block.time.nanos()
    );

    let withdraw = &res.messages[0].msg;
    match withdraw {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, &validator.address);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let swap = &res.messages[1].msg;
    match swap {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(msg, &to_binary(&SwapToRewardDenom {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", swap),
    }

    let update_g_index = &res.messages[2].msg;
    match update_g_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &reward_contract);
            assert_eq!(msg, &to_binary(&UpdateGlobalIndex {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", update_g_index),
    }
}

/// Covers update_global_index when there is more than one validator.
/// Checks if more than one Withdraw message is sent.
#[test]
pub fn proper_update_global_index_two_validators() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // bond
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(10),
        validator.clone(),
    );

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(10u128))])]);

    // register_validator
    do_register_validator(deps.as_mut(), validator2.clone());

    // bond to the second validator
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(10),
        validator2.clone(),
    );

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(10, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(10, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator.clone()), (validator2.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(20u128))])]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(4, res.messages.len());

    let withdraw = &res.messages[0].msg;
    match withdraw {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, &validator.address);
        }
        _ => panic!("Unexpected message: {:?}", withdraw),
    }

    let withdraw = &res.messages[1].msg;
    match withdraw {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, &validator2.address);
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
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // bond
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(10),
        validator.clone(),
    );

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(10u128))])]);

    // register_validator 2 but will not bond anything to it
    do_register_validator(deps.as_mut(), validator2);

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(10, "uluna")))];

    let validators: [Validator; 1] = [(validator.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(20u128))])]);

    let reward_msg = ExecuteMsg::UpdateGlobalIndex {
        airdrop_hooks: None,
    };

    let info = mock_info(&addr1, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());

    let withdraw = &res.messages[0].msg;
    match withdraw {
        CosmosMsg::Distribution(DistributionMsg::WithdrawDelegatorReward { validator: val }) => {
            assert_eq!(val, &validator.address);
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
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr0001".to_string();
    let invalid = "invalid".to_string();

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();
    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // bond to the second validator
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(10),
        validator.clone(),
    );
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(10u128))])]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::new(10),
        msg: to_binary(&"random").unwrap(),
    });

    let token_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), token_info, receive);
    assert!(res.is_err());

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128::new(10),
        msg: to_binary(&failed_unbond).unwrap(),
    });

    let invalid_info = mock_info(&invalid, &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, receive);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // successful call
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128::new(10),
        msg: to_binary(&successful_unbond).unwrap(),
    });

    let valid_info = mock_info(&token_contract, &[]);
    let res = execute(deps.as_mut(), mock_env(), valid_info, receive).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg = &res.messages[0].msg;
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Burn {
                    amount: Uint128::new(10)
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
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();
    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(10u128))])]);

    let res = execute(deps.as_mut(), mock_env(), info, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    //check the current batch before unbond
    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    let token_info = mock_info(&token_contract, &[]);
    let mut token_env = mock_env();

    // check the state before unbond
    let state = QueryMsg::State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(
        query_state.last_unbonded_time,
        mock_env().block.time.nanos()
    );
    assert_eq!(query_state.total_bond_amount, Uint128::new(1000010));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::new(1),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), mock_env(), token_info, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9u128))])]);

    //read the undelegated waitlist of the current epoch for the user bob
    let wait_list = read_unbond_wait_list(&deps.storage, 1, bob.clone()).unwrap();
    assert_eq!(Uint128::new(1), wait_list);

    //successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128::new(5),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let token_info = mock_info(&token_contract, &[]);

    let res = execute(
        deps.as_mut(),
        token_env.clone(),
        token_info.clone(),
        receive,
    )
    .unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(4u128))])]);

    let msg = &res.messages[0].msg;
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Burn {
                    amount: Uint128::new(5)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    let waitlist2 = read_unbond_wait_list(&deps.storage, 1, bob.to_string()).unwrap();
    assert_eq!(Uint128::new(6), waitlist2);

    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_with_fee, Uint128::new(6));

    token_env.block.time = token_env.block.time.plus_seconds(31);

    //pushing time forward to check the unbond message
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob,
        amount: Uint128::new(2),
        msg: to_binary(&successful_bond).unwrap(),
    });
    let res = execute(deps.as_mut(), token_env.clone(), token_info, receive).unwrap();
    assert_eq!(2, res.messages.len());

    let msg = &res.messages[1].msg;
    match msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Burn {
                    amount: Uint128::new(2)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", msg),
    }

    //making sure the sent message (2nd) is undelegate
    let msgs: SubMsg = SubMsg::new(CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    }));
    assert_eq!(res.messages[0], msgs);

    // check the current batch
    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    // check the state
    let state = QueryMsg::State {};
    let query_state: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();

    assert_eq!(query_state.last_unbonded_time, token_env.block.time.nanos());
    assert_eq!(query_state.total_bond_amount, Uint128::new(2));

    // the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = QueryMsg::UnbondRequests {
        address: "bob".to_string(),
    };
    let query_unbond: UnbondRequestsResponse =
        from_binary(&query(deps.as_ref(), mock_env(), waitlist).unwrap()).unwrap();
    assert_eq!(query_unbond.requests[0].0, 1);
    assert_eq!(query_unbond.requests[0].1, Uint128::new(8));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].amount, Uint128::new(8));
    assert_eq!(res.history[0].applied_exchange_rate, Decimal::one());
    assert!(!res.history[0].released);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the pick_validator function sends different Undelegate messages
/// to different validators, when a validator does not have enough delegation.
#[test]
pub fn proper_pick_validator() {
    let mut deps = dependencies(&[]);

    let addr1 = "addr1000".to_string();
    let addr2 = "addr2000".to_string();
    let addr3 = "addr3000".to_string();

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    let validator3 = sample_validator(DEFAULT_VALIDATOR3.to_string());

    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    do_register_validator(deps.as_mut(), validator.clone());
    do_register_validator(deps.as_mut(), validator2.clone());
    do_register_validator(deps.as_mut(), validator3.clone());

    // bond to a validator
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(10),
        validator.clone(),
    );
    do_bond(
        deps.as_mut(),
        addr2.clone(),
        Uint128::new(300),
        validator2.clone(),
    );
    do_bond(
        deps.as_mut(),
        addr3.clone(),
        Uint128::new(200),
        validator3.clone(),
    );

    // give validators different delegation amount
    let delegations: [FullDelegation; 3] = [
        (sample_delegation(validator.address.clone(), coin(10, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(300, "uluna"))),
        (sample_delegation(validator3.address.clone(), coin(200, "uluna"))),
    ];

    let validators: [Validator; 3] = [
        (validator.clone()),
        (validator2.clone()),
        (validator3.clone()),
    ];
    set_delegation_query(&mut deps.querier, &delegations, &validators);
    deps.querier.with_token_balances(&[(
        &"token".to_string(),
        &[
            (&addr3, &Uint128::new(200)),
            (&addr2, &Uint128::new(300)),
            (&addr1, &Uint128::new(10)),
        ],
    )]);

    // send the first burn
    let token_info = mock_info(&token_contract, &[]);
    let mut token_env = mock_env();

    let res = do_unbond(
        deps.as_mut(),
        addr2.clone(),
        token_env.clone(),
        token_info.clone(),
        Uint128::new(50),
    );
    assert_eq!(res.messages.len(), 1);

    deps.querier.with_token_balances(&[(
        &"token".to_string(),
        &[
            (&addr3, &Uint128::new(200)),
            (&addr2, &Uint128::new(250)),
            (&addr1, &Uint128::new(10)),
        ],
    )]);

    token_env.block.time = token_env.block.time.plus_seconds(40);

    // send the second burn
    let res = do_unbond(
        deps.as_mut(),
        addr2.clone(),
        token_env,
        token_info,
        Uint128::new(100),
    );
    assert!(res.messages.len() >= 2);

    deps.querier.with_token_balances(&[(
        &"token".to_string(),
        &[
            (&addr3, &Uint128::new(200)),
            (&addr2, &Uint128::new(150)),
            (&addr1, &Uint128::new(10)),
        ],
    )]);

    //check if the undelegate message is send two more than one validator.
    if res.messages.len() > 2 {
        match &res.messages[0].msg {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator.address {
                    assert_eq!(amount.amount, Uint128::new(10))
                }
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128::new(150))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128::new(150))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[1]),
        }

        match &res.messages[1].msg {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128::new(140))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128::new(140))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[2]),
        }
    } else {
        match &res.messages[1].msg {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128::new(150))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128::new(150))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[1]),
        }
    }
}

/// Covers if the pick_validator function sends different Undelegate messages
/// if the delegations of the user are distributed to several validators
/// and if the user wants to unbond amount that none of validators has.
#[test]
pub fn proper_pick_validator_respect_distributed_delegation() {
    let mut deps = dependencies(&[]);

    let addr1 = "addr1000".to_string();
    let addr2 = "addr2000".to_string();

    // create 3 validators
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    let validator2 = sample_validator(DEFAULT_VALIDATOR2.to_string());
    let validator3 = sample_validator(DEFAULT_VALIDATOR3.to_string());

    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        deps.borrow_mut(),
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    do_register_validator(deps.as_mut(), validator.clone());
    do_register_validator(deps.as_mut(), validator2.clone());
    do_register_validator(deps.as_mut(), validator3);

    // bond to a validator
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(1000),
        validator.clone(),
    );
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(1500),
        validator2.clone(),
    );

    // give validators different delegation amount
    let delegations: [FullDelegation; 2] = [
        (sample_delegation(validator.address.clone(), coin(1000, "uluna"))),
        (sample_delegation(validator2.address.clone(), coin(1500, "uluna"))),
    ];

    let validators: [Validator; 2] = [(validator.clone()), (validator2.clone())];
    set_delegation_query(&mut deps.querier, &delegations, &validators);

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(2500))])]);

    // send the first burn
    let token_info = mock_info(&token_contract, &[]);

    let mut token_env = mock_env();
    token_env.block.time = token_env.block.time.plus_seconds(40);

    let res = do_unbond(
        deps.as_mut(),
        addr2,
        token_env,
        token_info,
        Uint128::new(2000),
    );
    assert_eq!(res.messages.len(), 3);

    //check if the undelegate message is send two more than one validator.
    if res.messages.len() > 2 {
        match &res.messages[0].msg {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator.address {
                    assert_eq!(amount.amount, Uint128::new(1000))
                }
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128::new(1500))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[1]),
        }

        match &res.messages[1].msg {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator.address {
                    assert_eq!(amount.amount, Uint128::new(500))
                }
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128::new(1000))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[2]),
        }
    }
}

/// Covers the effect of slashing of bond, unbond, and withdraw_unbonded
/// update the exchange rate after and before slashing.
#[test]
pub fn proper_slashing() {
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();
    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    //bond
    do_bond(
        deps.as_mut(),
        addr1.clone(),
        Uint128::new(1000),
        validator.clone(),
    );

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(1000u128))])]);

    // slashing
    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let info = mock_info(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = execute(deps.as_mut(), mock_env(), info, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate.to_string(), "0.9");

    //bond again to see the update exchange rate
    let second_bond = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&addr1, &[coin(1000, "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    // expected exchange rate must be more than 0.9
    let expected_er = Decimal::from_ratio(Uint128::new(1900), Uint128::new(2111));
    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(1000, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let message = &res.messages[1].msg;
    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            funds: _,
        }) => {
            assert_eq!(contract_addr, &token_contract);
            assert_eq!(
                msg,
                &to_binary(&Mint {
                    recipient: info.sender.to_string(),
                    amount: Uint128::new(1111)
                })
                .unwrap()
            );
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    set_delegation(&mut deps.querier, validator.clone(), 100900, "uluna");

    //update user balance
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(2111u128))])]);

    let info = mock_info(&addr1, &[]);
    let mut env = mock_env();
    let _res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        Uint128::new(500),
        addr1.clone(),
    )
    .unwrap();

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(1611u128))])]);

    env.block.time = env.block.time.plus_seconds(31);

    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        Uint128::new(500),
        addr1.clone(),
    )
    .unwrap();
    let msgs: SubMsg = SubMsg::new(CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(900, "uluna"),
    }));
    assert_eq!(res.messages[0], msgs);

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &Uint128::new(1111u128))])]);

    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(900),
        },
    )]);

    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

    env.block.time = env.block.time.plus_seconds(90);
    //check withdrawUnbonded message
    let withdraw_unbond_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(deps.as_mut(), env, info, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

    let sent_message = &wdraw_unbonded_res.messages[0].msg;
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, &addr1);
            assert_eq!(amount[0].amount, Uint128::new(900))
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

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(100u128))])]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, 100, "uluna");

    let res = execute_unbond(
        deps.as_mut(),
        mock_env(),
        info,
        Uint128::new(10),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(90u128))])]);

    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(0),
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
    assert!(wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        Uint128::new(10),
        bob.clone(),
    )
    .unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(80u128))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::new(0));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(20),
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
    assert_eq!(res.requests[0].1, Uint128::new(20));
    assert_eq!(res.requests[0].0, 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].amount, Uint128::new(20));
    assert_eq!(res.history[0].batch_id, 1);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::new(20));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0].msg;
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128::new(20))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::new(0));

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
    let state = QueryMsg::State {};
    let state_query: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128::new(0));
    assert_eq!(state_query.exchange_rate, Decimal::one());
}

/// Covers slashing during the unbonded period and its effect on the finished amount.
#[test]
pub fn proper_withdraw_unbonded_respect_slashing() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::new(10000);
    let unbond_amount = Uint128::new(500);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &bond_amount)])]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond(deps.as_mut(), mock_env(), info, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9500))])]);

    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(0),
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
    assert!(wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        unbond_amount,
        bob.clone(),
    )
    .unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9000))])]);

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), mock_env(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::new(0));

    env.block.time = env.block.time.plus_seconds(91);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(900),
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
    assert_eq!(res.requests[0].1, Uint128::new(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::new(1000));

    let success_res = execute(deps.as_mut(), env, info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0].msg;
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128::new(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::new(0));
}

/// Covers withdraw_unbonded/inactivity in the system while there are slashing events.
#[test]
pub fn proper_withdraw_unbonded_respect_inactivity_slashing() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::new(10000);
    let unbond_amount = Uint128::new(500);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &bond_amount)])]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[0].msg;
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR.to_string());
            assert_eq!(amount, &coin(bond_amount.u128(), "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    set_delegation(&mut deps.querier, validator, bond_amount.u128(), "uluna");

    let res = execute_unbond(deps.as_mut(), mock_env(), info, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9500))])]);

    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(0),
        },
    )]);

    let info = mock_info(&bob, &[]);
    let mut env = mock_env();
    //set the block time 30 seconds from now.

    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_with_fee, unbond_amount);

    env.block.time = env.block.time.plus_seconds(1000);
    let wdraw_unbonded_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = execute(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        wdraw_unbonded_msg.clone(),
    );
    assert!(wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("No withdrawable uluna assets are available yet")
    );

    // trigger undelegation message
    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        unbond_amount,
        bob.clone(),
    )
    .unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9000))])]);

    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 2);
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), env.clone(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].amount, Uint128::new(1000));
    assert_eq!(res.history[0].withdraw_rate.to_string(), "1");
    assert!(!res.history[0].released);
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
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(900),
        },
    )]);
    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(deps.as_ref(), env.clone(), all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.requests.len(), 1);
    //the amount should be 10
    assert_eq!(&res.address, &bob);
    assert_eq!(res.requests[0].1, Uint128::new(1000));
    assert_eq!(res.requests[0].0, 1);

    //check with query
    //this query does not reflect the actual withdrawable
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
    };
    let query_with = query(deps.as_ref(), env.clone(), withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128::new(1000));

    let success_res = execute(deps.as_mut(), env.clone(), info, wdraw_unbonded_msg).unwrap();

    assert_eq!(success_res.messages.len(), 1);

    let sent_message = &success_res.messages[0].msg;
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, Uint128::new(899))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), env, withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::new(0));

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse =
        from_binary(&query(deps.as_ref(), mock_env(), all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].amount, Uint128::new(1000));
    assert_eq!(res.history[0].applied_exchange_rate.to_string(), "1");
    assert_eq!(res.history[0].withdraw_rate.to_string(), "0.899");
    assert!(res.history[0].released);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the signed integer works properly,
/// the exception when a user sends rogue coin.
#[test]
pub fn proper_withdraw_unbond_with_dummies() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let bond_amount = Uint128::new(10000);
    let unbond_amount = Uint128::new(500);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &bond_amount)])]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(
        &mut deps.querier,
        validator.clone(),
        bond_amount.u128(),
        "uluna",
    );

    let res = execute_unbond(deps.as_mut(), mock_env(), info, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9500))])]);

    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(0),
        },
    )]);

    let mut env = mock_env();
    let info = mock_info(&bob, &[]);
    //set the block time 30 seconds from now.

    env.block.time = env.block.time.plus_seconds(31);
    // trigger undelegation message
    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        unbond_amount,
        bob.clone(),
    )
    .unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(9000))])]);

    // slashing
    set_delegation(
        &mut deps.querier,
        validator,
        bond_amount.u128() - 2000,
        "uluna",
    );

    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        unbond_amount,
        bob.clone(),
    )
    .unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(8500))])]);

    env.block.time = env.block.time.plus_seconds(31);

    let res = execute_unbond(
        deps.as_mut(),
        env.clone(),
        info.clone(),
        unbond_amount,
        bob.clone(),
    )
    .unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &Uint128::new(8000))])]);

    // fabricate balance of the hub contract
    deps.querier.with_native_balances(&[(
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(2200),
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
    assert_eq!(res.history[0].amount, Uint128::new(1000));
    assert_eq!(res.history[0].withdraw_rate.to_string(), "1.164");
    assert!(res.history[0].released);
    assert_eq!(res.history[0].batch_id, 1);
    assert_eq!(res.history[1].amount, Uint128::new(1000));
    assert_eq!(res.history[1].withdraw_rate.to_string(), "1.033");
    assert!(res.history[1].released);
    assert_eq!(res.history[1].batch_id, 2);

    let expected = (res.history[0].withdraw_rate * res.history[0].amount)
        + res.history[1].withdraw_rate * res.history[1].amount;
    let sent_message = &success_res.messages[0].msg;
    match sent_message {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount[0].amount, expected)
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    // there should not be any result
    let withdrawable = WithdrawableUnbonded { address: bob };
    let query_with: WithdrawableUnbondedResponse =
        from_binary(&query(deps.as_ref(), mock_env(), withdrawable).unwrap()).unwrap();
    assert_eq!(query_with.withdrawable, Uint128::new(0));
}

/// Covers if the state/parameters storage is updated to the given value,
/// who sends the message, and if
/// RewardUpdateDenom message is sent to the reward contract
#[test]
pub fn test_update_params() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    //test with no swap denom.
    let update_prams = UpdateParams {
        epoch_period: Some(20),
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
    };
    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address,
    );

    let invalid_info = mock_info("invalid", &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        invalid_info,
        update_prams.clone(),
    );
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));
    let creator_info = mock_info("owner1", &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), QueryMsg::Parameters {}).unwrap()).unwrap();
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
    let creator_info = mock_info("owner1", &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), QueryMsg::Parameters {}).unwrap()).unwrap();
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
    let mut deps = dependencies(&[]);
    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let update_prams = UpdateParams {
        epoch_period: None,
        unbonding_period: None,
        peg_recovery_fee: Some(Decimal::from_ratio(Uint128::new(1), Uint128::new(1000))),
        er_threshold: Some(Decimal::from_ratio(Uint128::new(99), Uint128::new(100))),
    };
    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    let bond_amount = Uint128::new(1000000u128);
    let unbond_amount = Uint128::new(100000u128);

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract.clone(),
        validator.address.clone(),
    );

    let creator_info = mock_info("owner1", &[]);
    let res = execute(deps.as_mut(), mock_env(), creator_info, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let get_params = QueryMsg::Parameters {};
    let parmas: Parameters =
        from_binary(&query(deps.as_ref(), mock_env(), get_params).unwrap()).unwrap();
    assert_eq!(parmas.epoch_period, 30);
    assert_eq!(parmas.underlying_coin_denom, "uluna");
    assert_eq!(parmas.unbonding_period, 2);
    assert_eq!(parmas.peg_recovery_fee.to_string(), "0.001");
    assert_eq!(parmas.er_threshold.to_string(), "0.99");

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &bond_amount)])]);

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 900000, "uluna");

    let report_slashing = CheckSlashing {};
    let res = execute(deps.as_mut(), mock_env(), info, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate.to_string(), "0.9");

    //Bond again to see the applied result
    let bob = "bob".to_string();
    let bond_msg = ExecuteMsg::Bond {
        validator: validator.address.clone(),
    };

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &bond_amount)])]);

    let info = mock_info(&bob, &[coin(bond_amount.u128(), "uluna")]);

    let res = execute(deps.as_mut(), mock_env(), info, bond_msg).unwrap();
    let mint_amount = decimal_division(
        bond_amount,
        Decimal::from_ratio(Uint128::new(9), Uint128::new(10)),
    );
    let max_peg_fee = mint_amount * parmas.peg_recovery_fee;
    let required_peg_fee = ((bond_amount + mint_amount + Uint128::zero())
        .checked_sub(Uint128::new(900000) + bond_amount))
    .unwrap();
    let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
    let mint_amount_with_fee = (mint_amount.checked_sub(peg_fee)).unwrap();

    let mint_msg = &res.messages[1].msg;
    match mint_msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: _,
            msg,
            funds: _,
        }) => assert_eq!(
            msg,
            &to_binary(&Mint {
                recipient: bob.clone(),
                amount: mint_amount_with_fee
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

    let new_balance = bond_amount.checked_sub(unbond_amount).unwrap();

    let mut token_env = mock_env();
    let token_info = mock_info(&token_contract, &[]);
    let res = execute(
        deps.as_mut(),
        token_env.clone(),
        token_info.clone(),
        receive,
    )
    .unwrap();
    assert_eq!(1, res.messages.len());

    //check current batch
    let bonded_with_fee =
        unbond_amount * Decimal::from_ratio(Uint128::new(999), Uint128::new(1000));
    let current_batch = QueryMsg::CurrentBatch {};
    let query_batch: CurrentBatchResponse =
        from_binary(&query(deps.as_ref(), mock_env(), current_batch).unwrap()).unwrap();
    assert_eq!(query_batch.id, 1);
    assert_eq!(query_batch.requested_with_fee, bonded_with_fee);

    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&bob, &new_balance)])]);

    token_env.block.time = token_env.block.time.plus_seconds(60);

    let second_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract,
        amount: unbond_amount,
        msg: to_binary(&second_unbond).unwrap(),
    });
    let res = execute(
        deps.as_mut(),
        token_env.clone(),
        token_info.clone(),
        receive,
    )
    .unwrap();
    assert_eq!(2, res.messages.len());

    let ex_rate = QueryMsg::State {};
    let query_exchange_rate: StateResponse =
        from_binary(&query(deps.as_ref(), mock_env(), ex_rate).unwrap()).unwrap();
    let new_exchange = query_exchange_rate.exchange_rate;

    let expected = bonded_with_fee + bonded_with_fee;
    let undelegate_message = &res.messages[0].msg;
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
        MOCK_CONTRACT_ADDR.to_string(),
        Coin {
            denom: "uluna".to_string(),
            amount: Uint128::new(161870),
        },
    )]);

    token_env.block.time = token_env.block.time.plus_seconds(90);
    //check withdrawUnbonded message
    let withdraw_unbond_msg = ExecuteMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res =
        execute(deps.as_mut(), token_env, token_info, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let sent_message = &wdraw_unbonded_res.messages[0].msg;
    let expected = ((expected
        * new_exchange
        * Decimal::from_ratio(Uint128::new(161870), expected * new_exchange))
    .checked_sub(Uint128::new(1)))
    .unwrap();
    match sent_message {
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
    assert_eq!(res.history[0].amount, bonded_with_fee + bonded_with_fee);
    assert_eq!(res.history[0].applied_exchange_rate, new_exchange);
    assert_eq!(
        res.history[0].withdraw_rate,
        Decimal::from_ratio(Uint128::new(161869), bonded_with_fee + bonded_with_fee)
    );
    assert!(res.history[0].released);
    assert_eq!(res.history[0].batch_id, 1);
}

/// Covers if the storage affected by update_config are updated properly
#[test]
pub fn proper_update_config() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let new_owner = "new_owner".to_string();
    let invalid_owner = "invalid_owner".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();
    let airdrop_registry = "airdrop_registry".to_string();

    init(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract.clone(),
        validator.address,
    );

    let config = QueryMsg::Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(&config_query.token_contract.unwrap(), &token_contract);
    assert_eq!(
        &config_query.airdrop_registry_contract.unwrap(),
        &airdrop_registry
    );

    //make sure the other configs are still the same.
    assert_eq!(&config_query.reward_contract.unwrap(), &reward_contract);
    assert_eq!(&config_query.owner, &owner);

    // only the owner can call this message
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        reward_contract: None,
        token_contract: None,
        airdrop_registry_contract: None,
    };
    let info = mock_info(&invalid_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), info, update_config);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    // change the owner
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        reward_contract: None,
        token_contract: None,
        airdrop_registry_contract: None,
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
    };

    let new_owner_info = mock_info(&owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_prams);
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let update_config = UpdateConfig {
        owner: None,
        reward_contract: Some("new reward".to_string()),
        token_contract: None,
        airdrop_registry_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 1);

    let msg: SubMsg = SubMsg::new(CosmosMsg::Distribution(
        DistributionMsg::SetWithdrawAddress {
            address: "new reward".to_string(),
        },
    ));
    assert_eq!(msg, res.messages[0]);

    let config = QueryMsg::Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.reward_contract.unwrap(),
        "new reward".to_string()
    );

    let update_config = UpdateConfig {
        owner: None,
        reward_contract: None,
        token_contract: Some("new token".to_string()),
        airdrop_registry_contract: None,
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = QueryMsg::Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.token_contract.unwrap(),
        "new token".to_string()
    );

    //make sure the other configs are still the same.
    assert_eq!(
        config_query.reward_contract.unwrap(),
        "new reward".to_string()
    );
    assert_eq!(config_query.owner, new_owner);

    let update_config = UpdateConfig {
        owner: None,
        reward_contract: None,
        token_contract: None,
        airdrop_registry_contract: Some("new airdrop".to_string()),
    };
    let new_owner_info = mock_info(&new_owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), new_owner_info, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = QueryMsg::Config {};
    let config_query: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), config).unwrap()).unwrap();
    assert_eq!(
        config_query.airdrop_registry_contract.unwrap(),
        "new airdrop".to_string()
    );
}

#[test]
fn proper_claim_airdrop() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();
    let airdrop_registry = "airdrop_registry".to_string();

    init(
        &mut deps,
        owner.clone(),
        reward_contract,
        token_contract,
        validator.address,
    );

    let claim_msg = ExecuteMsg::ClaimAirdrop {
        airdrop_token_contract: "airdrop_token".to_string(),
        airdrop_contract: "MIR_contract".to_string(),
        airdrop_swap_contract: "airdrop_swap".to_string(),
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
        res.messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "MIR_contract".to_string(),
            msg: to_binary(&MIRMsg::MIRClaim {}).unwrap(),
            funds: vec![]
        }))
    );
    assert_eq!(
        res.messages[1],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: MOCK_CONTRACT_ADDR.to_string(),
            msg: to_binary(&ExecuteMsg::SwapHook {
                airdrop_token_contract: "airdrop_token".to_string(),
                airdrop_swap_contract: "airdrop_swap".to_string(),
                swap_msg: Default::default()
            })
            .unwrap(),
            funds: vec![]
        }))
    );
}

#[test]
fn proper_swap_hook() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract,
        validator.address,
    );

    let swap_msg = ExecuteMsg::SwapHook {
        airdrop_token_contract: "airdrop_token".to_string(),
        airdrop_swap_contract: "swap_contract".to_string(),
        swap_msg: to_binary(&PairHandleMsg::Swap {
            belief_price: None,
            max_spread: None,
            to: Some(reward_contract.clone()),
        })
        .unwrap(),
    };

    //invalid sender
    let info = mock_info(&owner, &[]);
    let env = mock_env();
    let res = execute(deps.as_mut(), mock_env(), info, swap_msg.clone()).unwrap_err();
    assert_eq!(res, StdError::generic_err("unauthorized"));

    // no balance for hub
    let contract_info = mock_info(env.contract.address.as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        mock_env(),
        contract_info.clone(),
        swap_msg.clone(),
    )
    .unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err(format!(
            "There is no balance for {} in airdrop token contract {}",
            &env.contract.address,
            &"airdrop_token".to_string()
        ))
    );

    deps.querier.with_token_balances(&[(
        &"airdrop_token".to_string(),
        &[(&env.contract.address.to_string(), &Uint128::new(1000))],
    )]);

    let res = execute(deps.as_mut(), mock_env(), contract_info, swap_msg).unwrap();
    assert_eq!(res.messages.len(), 1);
    assert_eq!(
        res.messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "airdrop_token".to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: "swap_contract".to_string(),
                amount: Uint128::new(1000),
                msg: to_binary(&PairHandleMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: Some(reward_contract),
                })
                .unwrap()
            })
            .unwrap(),
            funds: vec![],
        }))
    )
}

#[test]
fn proper_update_global_index_with_airdrop() {
    let mut deps = dependencies(&[]);

    let validator = sample_validator(DEFAULT_VALIDATOR.to_string());
    set_validator_mock(&mut deps.querier);

    let addr1 = "addr1000".to_string();
    let bond_amount = Uint128::new(10);

    let owner = "owner1".to_string();
    let token_contract = "token".to_string();
    let reward_contract = "reward".to_string();

    init(
        &mut deps,
        owner,
        reward_contract,
        token_contract,
        validator.address.clone(),
    );

    // register_validator
    do_register_validator(deps.as_mut(), validator.clone());

    // bond
    do_bond(deps.as_mut(), addr1.clone(), bond_amount, validator.clone());

    //set delegation for query-all-delegation
    let delegations: [FullDelegation; 1] =
        [(sample_delegation(validator.address.clone(), coin(bond_amount.u128(), "uluna")))];

    let validators: [Validator; 1] = [(validator)];

    set_delegation_query(&mut deps.querier, &delegations, &validators);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&"token".to_string(), &[(&addr1, &bond_amount)])]);

    let binary_msg = to_binary(&FabricateMIRClaim {
        stage: 0,
        amount: Uint128::new(1000),
        proof: vec!["proof".to_string()],
    })
    .unwrap();

    let binary_msg2 = to_binary(&FabricateANCClaim {
        stage: 0,
        amount: Uint128::new(1000),
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
        res.messages[0],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "airdrop_registry".to_string(),
            msg: binary_msg,
            funds: vec![],
        }))
    );

    assert_eq!(
        res.messages[1],
        SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: "airdrop_registry".to_string(),
            msg: binary_msg2,
            funds: vec![],
        }))
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
    let accumulated_rewards = coin(0, &amount.denom);
    FullDelegation {
        validator: addr,
        delegator: Addr::unchecked(MOCK_CONTRACT_ADDR),
        amount,
        can_redelegate,
        accumulated_rewards: vec![accumulated_rewards],
    }
}

// sample MIR claim msg
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MIRMsg {
    MIRClaim {},
}
