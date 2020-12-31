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
    coin, from_binary, to_binary, Api, BankMsg, Coin, CosmosMsg, Decimal, Env, Extern,
    FullDelegation, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg, StdError,
    Storage, Uint128, Validator, WasmMsg,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env};

use crate::msg::{
    AllHistoryResponse, ConfigResponse, CurrentBatchResponse, InitMsg, StateResponse,
    UnbondBatchesResponse, UnbondRequestsResponse, WhitelistedValidatorsResponse,
    WithdrawableUnbondedResponse,
};
use hub_querier::HandleMsg;

use crate::contract::{handle, init, query};
use crate::unbond::handle_unbond;

use anchor_basset_reward::msg::HandleMsg::UpdateRewardDenom;

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg};
use cw20_base::msg::HandleMsg::{Burn, Mint};
use hub_querier::Cw20HookMsg::Unbond;
use hub_querier::HandleMsg::{CheckSlashing, Receive, UpdateConfig, UpdateParams};
use hub_querier::Registration::{Reward, Token};

use super::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
use crate::math::decimal_division;
use crate::msg::QueryMsg::{
    AllHistory, Config, CurrentBatch, Parameters as Params, State, UnbondBatches, UnbondRequests,
    WhitelistedValidators, WithdrawableUnbonded,
};
use crate::state::{read_config, read_unbond_wait_list, Parameters};
use anchor_basset_reward::msg::HandleMsg::{SwapToRewardDenom, UpdateGlobalIndex};

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2000";
const DEFAULT_VALIDATOR3: &str = "default-validator3000";

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

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
    token_contract: HumanAddr,
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

    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Reward,
        contract_address: reward_contract,
    };
    handle(&mut deps, owner_env.clone(), register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Token,
        contract_address: token_contract,
    };
    handle(&mut deps, owner_env, register_msg).unwrap();
}

pub fn do_register_validator<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    validator: Validator,
) {
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address,
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());
}

pub fn do_bond<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr: HumanAddr,
    amount: Uint128,
    validator: Validator,
) {
    let bond = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&addr, &[coin(amount.0, "uluna")]);
    let res = handle(&mut deps, env, bond).unwrap();
    assert_eq!(2, res.messages.len());
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
    let mut deps = mock_dependencies(20, &[]);

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
        exchange_rate: Decimal::one(),
        total_bond_amount: Default::default(),
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
        reward_contract: None,
        token_contract: None,
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
            requested_with_fee: Default::default()
        }
    );
}

/// Covers if subcontracts are stored in config storage.
#[test]
fn proper_register_subcontracts() {
    let mut deps = mock_dependencies(20, &[]);

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
    init(&mut deps, owner_env.clone(), msg).unwrap();

    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let invalid_sender = HumanAddr::from("invalid");
    let invalid_env = mock_env(invalid_sender, &[]);

    // unauthorized call
    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Reward,
        contract_address: reward_contract.clone(),
    };
    let res = handle(&mut deps, invalid_env, register_msg).unwrap_err();
    assert_eq!(res, StdError::unauthorized());

    // register reward contract
    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Reward,
        contract_address: reward_contract.clone(),
    };
    let res = handle(&mut deps, owner_env.clone(), register_msg).unwrap();
    assert_eq!(res.messages.len(), 1);

    // register reward contract sends a Withdraw message
    let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
        validator: HumanAddr::default(),
        recipient: Some(reward_contract.clone()),
    });
    assert_eq!(msg, res.messages[0]);

    // register token contract
    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Token,
        contract_address: token_contract.clone(),
    };
    let res = handle(&mut deps, owner_env.clone(), register_msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // should not be registered twice
    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Reward,
        contract_address: reward_contract.clone(),
    };
    let res = handle(&mut deps, owner_env.clone(), register_msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("The reward contract is already registered",)
    );

    let register_msg = HandleMsg::RegisterSubcontracts {
        contract: Token,
        contract_address: token_contract.clone(),
    };
    let res = handle(&mut deps, owner_env, register_msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("The token contract is already registered",)
    );

    // check if they are store in config
    let conf = Config {};
    let query_conf: ConfigResponse = from_binary(&query(&deps, conf).unwrap()).unwrap();
    let expected_conf = ConfigResponse {
        owner: HumanAddr::from("owner1"),
        reward_contract: Some(reward_contract),
        token_contract: Some(token_contract),
    };

    assert_eq!(expected_conf, query_conf)
}

/// Covers if a given validator is registered in whitelisted validator storage.
#[test]
fn proper_register_validator() {
    let mut deps = dependencies(20, &[]);

    // first need to have validators
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // send by invalid user
    let owner = HumanAddr::from("invalid");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    // invalid requests
    let res = handle(&mut deps, owner_env, msg);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    //invalid validator
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: HumanAddr::from("invalid validator"),
    };

    let res = handle(&mut deps, owner_env, msg);
    assert_eq!(res.unwrap_err(), StdError::generic_err("Invalid validator"));

    // successful call
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let query_validatator = WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(&deps, query_validatator).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(0).unwrap(), &validator.address);

    // register another validator
    let msg = HandleMsg::RegisterValidator {
        validator: validator2.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // check if the validator is sored;
    let query_validatator2 = WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(&deps, query_validatator2).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(1).unwrap(), &validator2.address);
    assert_eq!(query_res.validators.get(0).unwrap(), &validator.address);
}

/// Covers if delegate message is sent to the specified validator,
/// mint message is sent to the token contract, state is changed based on new mint,
/// and check unsuccessful calls, like unsupported validators, and invalid coin.
#[test]
fn proper_bond() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");
    let bond_amount = Uint128(10000);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bond_msg = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&addr1, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    // set bob's balance in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &bond_amount)])]);

    let delegate = &res.messages[0];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(bond_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let mint = &res.messages[1];
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
                    recipient: addr1,
                    amount: bond_amount
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", mint),
    }

    // get total bonded
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.total_bond_amount, bond_amount);
    assert_eq!(query_state.exchange_rate, Decimal::one());

    //test unsupported validator
    let invalid_validator = sample_validator("invalid");
    let bob = HumanAddr::from("bob");
    let bond = HandleMsg::Bond {
        validator: invalid_validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);
    let res = handle(&mut deps, env, bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Unsupported validator")
    );

    // no-send funds
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna tokens sent")
    );

    //send other tokens than luna funds
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let bob = HumanAddr::from("bob");
    let failed_bond = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "ukrt")]);
    let res = handle(&mut deps, env, failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna tokens sent")
    );
}

/// Covers if the Redelegate message and UpdateGlobalIndex are sent.
/// It also checks if the validator is removed from the storage.
#[test]
fn proper_deregister() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let delegated_amount = Uint128(10);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner.clone(), reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // register_validator2
    do_register_validator(&mut deps, validator2.clone());

    set_delegation(
        &mut deps.querier,
        validator.clone(),
        delegated_amount.0,
        "uluna",
    );

    // check invalid sender
    let msg = HandleMsg::DeregisterValidator {
        validator: validator.address.clone(),
    };

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, msg);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let msg = HandleMsg::DeregisterValidator {
        validator: validator.address.clone(),
    };

    let owner_env = mock_env(owner, &[]);
    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(2, res.messages.len());

    let redelegate_msg = &res.messages[0];
    match redelegate_msg {
        CosmosMsg::Staking(StakingMsg::Redelegate {
            src_validator,
            dst_validator,
            amount,
        }) => {
            assert_eq!(src_validator.0, DEFAULT_VALIDATOR);
            assert_eq!(dst_validator.0, DEFAULT_VALIDATOR2);
            assert_eq!(amount, &coin(delegated_amount.0, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", redelegate_msg),
    }

    let global_index = &res.messages[1];
    match global_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr.0, MOCK_CONTRACT_ADDR);
            assert_eq!(msg, &to_binary(&HandleMsg::UpdateGlobalIndex {}).unwrap())
        }
        _ => panic!("Unexpected message: {:?}", redelegate_msg),
    }

    let query_validator = WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(&deps, query_validator).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(0).unwrap(), &validator2.address);
    assert_eq!(query_res.validators.contains(&validator.address), false);
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract.clone(), token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), bond_amount, validator.clone());

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &bond_amount)])]);

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

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
            assert_eq!(msg, &to_binary(&SwapToRewardDenom {}).unwrap())
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
            assert_eq!(
                msg,
                &to_binary(&UpdateGlobalIndex {
                    prev_balance: Uint128(2000)
                })
                .unwrap()
            )
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator.clone());

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    // register_validator
    do_register_validator(&mut deps, validator2.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator2.clone());

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(20u128))])]);

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

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
    let reward_contract = HumanAddr::from("reward");
    initialize(&mut deps, owner, reward_contract, token_contract.clone());

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator.clone());
    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(10),
        msg: None,
    });

    let token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env, receive);
    assert_eq!(res.unwrap_err(), StdError::generic_err("Invalid request"));

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
    let reward_contract = HumanAddr::from("reward");
    initialize(&mut deps, owner, reward_contract, token_contract.clone());

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

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
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    let token_env = mock_env(&token_contract, &[]);

    // check the state before unbond
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_amount, Uint128(10));

    // successful call
    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(1),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env, receive).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9u128))])]);

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
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(4u128))])]);

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
    assert_eq!(query_batch.requested_with_fee, Uint128(6));

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
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    // check the state
    let state = State {};
    let query_state: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(query_state.last_unbonded_time, token_env.block.time);
    assert_eq!(query_state.total_bond_amount, Uint128(2));

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
    assert_eq!(res.history[0].1.amount, Uint128(8));
    assert_eq!(res.history[0].1.released, false);
    assert_eq!(res.history[0].0, 1);
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract.clone());

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3.clone());

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator.clone());
    do_bond(&mut deps, addr2.clone(), Uint128(300), validator2.clone());
    do_bond(&mut deps, addr3.clone(), Uint128(200), validator3.clone());

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
    set_delegation_pick(&mut deps.querier, &delegations, &validators);
    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[
            (&addr3, &Uint128(200)),
            (&addr2, &Uint128(300)),
            (&addr1, &Uint128(10)),
        ],
    )]);

    // send the first burn
    let mut token_env = mock_env(&token_contract, &[]);
    let res = do_unbond(&mut deps, addr2.clone(), token_env.clone(), Uint128(50));
    assert_eq!(res.messages.len(), 1);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[
            (&addr3, &Uint128(200)),
            (&addr2, &Uint128(250)),
            (&addr1, &Uint128(10)),
        ],
    )]);

    token_env.block.time += 40;

    // send the second burn
    let res = do_unbond(&mut deps, addr2.clone(), token_env, Uint128(100));
    assert!(res.messages.len() >= 2);

    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[
            (&addr3, &Uint128(200)),
            (&addr2, &Uint128(150)),
            (&addr1, &Uint128(10)),
        ],
    )]);

    //check if the undelegate message is send two more than one validator.
    if res.messages.len() > 2 {
        match &res.messages[0] {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator.address {
                    assert_eq!(amount.amount, Uint128(10))
                }
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128(150))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128(150))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[1]),
        }

        match &res.messages[1] {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128(140))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128(140))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[2]),
        }
    } else {
        match &res.messages[1] {
            CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: val,
                amount,
            }) => {
                if val == &validator2.address {
                    assert_eq!(amount.amount, Uint128(150))
                }
                if val == &validator3.address {
                    assert_eq!(amount.amount, Uint128(150))
                }
            }
            _ => panic!("Unexpected message: {:?}", &res.messages[1]),
        }
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
    let reward_contract = HumanAddr::from("reward");
    initialize(&mut deps, owner, reward_contract, token_contract.clone());

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    //bond
    do_bond(&mut deps, addr1.clone(), Uint128(1000), validator.clone());

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(1000u128))])]);

    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let env = mock_env(&addr1, &[]);
    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate.to_string(), "0.9");

    //bond again to see the update exchange rate
    let second_bond = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&addr1, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    // expected exchange rate must be more than 0.9
    let expected_er = Decimal::from_ratio(Uint128(1900), Uint128(2111));
    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

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
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(2111u128))])]);

    let mut env = mock_env(&addr1, &[]);
    let _res = handle_unbond(&mut deps, env.clone(), Uint128(500), addr1.clone()).unwrap();

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(1611u128))])]);

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
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

    env.block.time += 90;
    //check withdrawUnbonded message
    let withdraw_unbond_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env, withdraw_unbond_msg).unwrap();
    assert_eq!(wdraw_unbonded_res.messages.len(), 1);

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate, expected_er);

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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(100u128))])]);

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

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(90u128))])]);

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
        StdError::generic_err("Previously requested amount is not ready yet")
    );

    let res = handle_unbond(&mut deps, env.clone(), Uint128(10), bob.clone()).unwrap();
    assert_eq!(res.messages.len(), 2);
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(80u128))])]);

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

    //first query AllUnbondedEpochs
    let all_user_epochs = UnbondBatches {
        address: bob.clone(),
    };

    let query_epochs = query(&deps, all_user_epochs).unwrap();
    let res: UnbondBatchesResponse = from_binary(&query_epochs).unwrap();
    assert_eq!(res.unbond_batches.len(), 1);
    //the epoch should be zero
    assert_eq!(res.unbond_batches[0], 1);

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].1.amount, Uint128(20));
    assert_eq!(res.history[0].0, 1);

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
            address: bob.clone(),
            requests: vec![]
        }
    );

    let batches = UnbondBatches { address: bob };
    let query_batches: UnbondBatchesResponse =
        from_binary(&query(&deps, batches).unwrap()).unwrap();
    assert_eq!(
        query_batches,
        UnbondBatchesResponse {
            unbond_batches: vec![]
        }
    );

    let state = State {};
    let state_query: StateResponse = from_binary(&query(&deps, state).unwrap()).unwrap();
    assert_eq!(state_query.prev_hub_balance, Uint128::zero());
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &bond_amount)])]);

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
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9500))])]);

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
        StdError::generic_err("Previously requested amount is not ready yet")
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

    //first query AllUnbondedEpochs
    let all_user_epochs = UnbondBatches {
        address: bob.clone(),
    };

    let query_epochs = query(&deps, all_user_epochs).unwrap();
    let res: UnbondBatchesResponse = from_binary(&query_epochs).unwrap();
    assert_eq!(res.unbond_batches.len(), 1);
    //the epoch should be zero
    assert_eq!(res.unbond_batches[0], 1);

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
            assert_eq!(amount[0].amount, Uint128(900))
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &bond_amount)])]);

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

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9500))])]);

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
    assert_eq!(query_batch.requested_with_fee, unbond_amount);

    env.block.time += 1000;
    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());
    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("Previously requested amount is not ready yet")
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
    assert_eq!(query_batch.requested_with_fee, Uint128::zero());

    let all_batches = AllHistory {
        start_from: None,
        limit: None,
    };
    let res: AllHistoryResponse = from_binary(&query(&deps, all_batches).unwrap()).unwrap();
    assert_eq!(res.history[0].1.amount, Uint128(1000));
    assert_eq!(res.history[0].1.withdraw_rate.to_string(), "1");
    assert_eq!(res.history[0].1.released, false);
    assert_eq!(res.history[0].0, 1);

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

    //first query AllUnbondedEpochs
    let all_user_epochs = UnbondBatches {
        address: bob.clone(),
    };

    let query_epochs = query(&deps, all_user_epochs).unwrap();
    let res: UnbondBatchesResponse = from_binary(&query_epochs).unwrap();
    assert_eq!(res.unbond_batches.len(), 1);
    //the epoch should be zero
    assert_eq!(res.unbond_batches[0], 1);

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
            assert_eq!(amount[0].amount, Uint128(900))
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
    assert_eq!(res.history[0].1.amount, Uint128(1000));
    assert_eq!(res.history[0].1.withdraw_rate.to_string(), "0.9");
    assert_eq!(res.history[0].1.released, true);
    assert_eq!(res.history[0].0, 1);
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
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &bond_amount)])]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), bond_amount.0, "uluna");

    let res = handle_unbond(&mut deps, env, unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9500))])]);

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
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(9000))])]);

    // slashing
    set_delegation(&mut deps.querier, validator, bond_amount.0 - 2000, "uluna");

    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(8500))])]);

    env.block.time += 31;
    let res = handle_unbond(&mut deps, env.clone(), unbond_amount, bob.clone()).unwrap();
    assert_eq!(2, res.messages.len());
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(8000))])]);

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
    assert_eq!(res.history[0].1.amount, Uint128(1000));
    assert_eq!(res.history[0].1.withdraw_rate.to_string(), "1.165");
    assert_eq!(res.history[0].1.released, true);
    assert_eq!(res.history[0].0, 1);
    assert_eq!(res.history[1].1.amount, Uint128(1000));
    assert_eq!(res.history[1].1.withdraw_rate.to_string(), "1.034");
    assert_eq!(res.history[1].1.released, true);
    assert_eq!(res.history[1].0, 2);

    let expected = (res.history[0].1.withdraw_rate * res.history[0].1.amount)
        + res.history[1].1.withdraw_rate * res.history[1].1.amount;
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
    //test with no swap denom.
    let update_prams = UpdateParams {
        epoch_period: Some(20),
        underlying_coin_denom: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    initialize(&mut deps, owner, reward_contract.clone(), token_contract);

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
        underlying_coin_denom: None,
        unbonding_period: Some(3),
        peg_recovery_fee: Some(Decimal::one()),
        er_threshold: Some(Decimal::zero()),
        reward_denom: Some("ukrw".to_string()),
    };

    //the result must be 1
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(
        res.messages,
        vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_contract,
            send: vec![],
            msg: to_binary(&UpdateRewardDenom {
                reward_denom: Some("ukrw".to_string()),
            })
            .unwrap()
        })]
    );

    let params: Parameters = from_binary(&query(&deps, Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_period, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.unbonding_period, 3);
    assert_eq!(params.peg_recovery_fee, Decimal::one());
    assert_eq!(params.er_threshold, Decimal::zero());
    assert_eq!(params.reward_denom, "ukrw");
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
        underlying_coin_denom: None,
        unbonding_period: None,
        peg_recovery_fee: Some(Decimal::from_ratio(Uint128(1), Uint128(1000))),
        er_threshold: Some(Decimal::from_ratio(Uint128(99), Uint128(100))),
        reward_denom: None,
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    let bond_amount = Uint128(1000000u128);
    let unbond_amount = Uint128(100000u128);

    initialize(&mut deps, owner, reward_contract, token_contract.clone());

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
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &bond_amount)])]);

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 900000, "uluna");

    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = State {};
    let query_exchange_rate: StateResponse = from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.exchange_rate.to_string(), "0.9");

    //Bond again to see the applied result
    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &bond_amount)])]);

    let env = mock_env(&bob, &[coin(bond_amount.0, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    let expected = decimal_division(bond_amount, Decimal::from_ratio(Uint128(9), Uint128(10)))
        * Decimal::from_ratio(Uint128(999), Uint128(1000));
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
                amount: expected
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
    assert_eq!(query_batch.requested_with_fee, bonded_with_fee);

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &new_balance)])]);

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
    let new_exchange = query_exchange_rate.exchange_rate;

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
        expected * new_exchange * Decimal::from_ratio(Uint128(161870), expected * new_exchange);
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
    assert_eq!(res.history[0].1.amount, bonded_with_fee + bonded_with_fee);
    assert_eq!(
        res.history[0].1.withdraw_rate,
        Decimal::from_ratio(Uint128(161870), bonded_with_fee + bonded_with_fee)
    );
    assert_eq!(res.history[0].1.released, true);
    assert_eq!(res.history[0].0, 1);
}

/// Covers if the storage affected by update_config are updated properly
#[test]
pub fn proper_update_config() {
    let mut deps = dependencies(20, &[]);

    let owner = HumanAddr::from("owner1");
    let new_owner = HumanAddr::from("new_owner");
    let invalid_owner = HumanAddr::from("invalid_owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    initialize(
        &mut deps,
        owner.clone(),
        reward_contract.clone(),
        token_contract.clone(),
    );

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(&config_query.token_contract.unwrap(), &token_contract);

    //make sure the other configs are still the same.
    assert_eq!(&config_query.reward_contract.unwrap(), &reward_contract);
    assert_eq!(&config_query.owner, &owner);

    // only the owner can call this message
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        reward_contract: None,
        token_contract: None,
    };
    let env = mock_env(&invalid_owner, &[]);
    let res = handle(&mut deps, env, update_config);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // change the owner
    let update_config = UpdateConfig {
        owner: Some(new_owner.clone()),
        reward_contract: None,
        token_contract: None,
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
        underlying_coin_denom: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };

    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    //previous owner cannot send this message
    let update_prams = UpdateParams {
        epoch_period: None,
        underlying_coin_denom: None,
        unbonding_period: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };

    let new_owner_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let update_config = UpdateConfig {
        owner: None,
        reward_contract: Some(HumanAddr::from("new reward")),
        token_contract: None,
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
        config_query.reward_contract.unwrap(),
        HumanAddr::from("new reward")
    );

    let update_config = UpdateConfig {
        owner: None,
        reward_contract: None,
        token_contract: Some(HumanAddr::from("new token")),
    };
    let new_owner_env = mock_env(&new_owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = Config {};
    let config_query: ConfigResponse = from_binary(&query(&deps, config).unwrap()).unwrap();
    assert_eq!(
        config_query.token_contract.unwrap(),
        HumanAddr::from("new token")
    );

    //make sure the other configs are still the same.
    assert_eq!(
        config_query.reward_contract.unwrap(),
        HumanAddr::from("new reward")
    );
    assert_eq!(config_query.owner, new_owner);
}

fn set_delegation(querier: &mut WasmMockQuerier, validator: Validator, amount: u128, denom: &str) {
    querier.update_staking(
        "uluna",
        &[validator.clone()],
        &[sample_delegation(validator.address, coin(amount, denom))],
    );
}

fn set_delegation_pick(
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
