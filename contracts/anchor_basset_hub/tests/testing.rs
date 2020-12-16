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
    coin, from_binary, to_binary, Api, BankMsg, CanonicalAddr, Coin, CosmosMsg, Decimal, Env,
    Extern, FullDelegation, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg, StdError,
    StdResult, Storage, Uint128, Validator, WasmMsg,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env};

use anchor_basset_hub::msg::{
    ExchangeRateResponse, InitMsg, TotalBondedResponse, UnbondEpochsResponse,
    UnbondRequestsResponse, WhitelistedValidatorsResponse, WithdrawableUnbondedResponse,
};
use gov_courier::{Deactivated, HandleMsg, PoolInfo};

use anchor_basset_hub::contract::{handle, init, query};
use anchor_basset_hub::unbond::handle_unbond;

use anchor_basset_reward::contracts::init as reward_init;
use anchor_basset_reward::msg::{
    HandleMsg::UpdateParams as RewardUpdateParams, InitMsg as RewardInitMsg,
};
use anchor_basset_token::contract::{
    handle as token_handle, init as token_init, query as token_query,
};

use anchor_basset_token::msg::TokenInitMsg;
use cosmwasm_storage::Singleton;
use cw20::{BalanceResponse, Cw20HandleMsg, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};
use cw20_base::msg::HandleMsg::{Burn, Mint, Send};
use cw20_base::msg::QueryMsg::{Balance, TokenInfo};
use gov_courier::Cw20HookMsg::Unbond;
use gov_courier::HandleMsg::{CheckSlashing, DeactivateMsg, Receive, UpdateConfig, UpdateParams};
use gov_courier::Registration::{Reward, Token};

mod common;
use anchor_basset_hub::msg::QueryMsg::{
    ExchangeRate, Parameters as Params, TotalBonded, UnbondEpochs, UnbondRequests,
    WhitelistedValidators, WithdrawableUnbonded,
};
use anchor_basset_hub::state::{
    config_read, epoch_read, get_all_delegations, get_bonded, read_total_amount,
    read_undelegated_wait_list, Parameters,
};
use anchor_basset_reward::msg::HandleMsg::{DecreaseBalance, SwapToRewardDenom, UpdateGlobalIndex};
use common::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2000";
const DEFAULT_VALIDATOR3: &str = "default-validator3000";

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub static POOL_INFO: &[u8] = b"pool_info";
pub static CONFIG: &[u8] = b"config";

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

fn default_token(hub_contract: HumanAddr, minter: HumanAddr) -> TokenInitMsg {
    TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse { minter, cap: None }),
        hub_contract,
    }
}

fn default_reward(hub_contract: HumanAddr, reward_denom: String) -> RewardInitMsg {
    RewardInitMsg {
        hub_contract,
        reward_denom,
    }
}

pub fn init_all<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    owner: HumanAddr,
    reward_contract: HumanAddr,
    token_contract: HumanAddr,
) {
    let msg = InitMsg {
        epoch_time: 30,
        underlying_coin_denom: "uluna".to_string(),
        undelegated_epoch: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let gov_env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
    let env = mock_env(owner.clone(), &[]);
    init(&mut deps, env, msg).unwrap();

    let reward_in = default_reward(HumanAddr::from(MOCK_CONTRACT_ADDR), "uusd".to_string());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(HumanAddr::from(MOCK_CONTRACT_ADDR), owner);
    token_init(&mut deps, gov_env, token_int).unwrap();

    let register_msg = HandleMsg::RegisterSubcontracts { contract: Reward };
    let register_env = mock_env(reward_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubcontracts { contract: Token };
    let register_env = mock_env(token_contract, &[]);
    handle(&mut deps, register_env, register_msg).unwrap();
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
    let _res = handle(&mut deps, env, bond);
    let msg = Mint {
        recipient: addr,
        amount,
    };

    let owner = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let env = mock_env(&owner, &[]);
    let res = token_handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
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

#[test]
fn proper_exchange_rate() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr100");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    //check exchange_rate when total bonded is zero
    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "1");

    do_register_validator(&mut deps, validator.clone());

    do_bond(&mut deps, addr1.clone(), Uint128(1000), validator.clone());

    //this will set the balance of the user in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(1000u128))])]);

    //check exchange_rate when total bonded is equal to total issued
    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "1");

    set_delegation(&mut deps.querier, validator, 900, "uluna");

    let env = mock_env(&owner, &[coin(1000u128, "uluna")]);
    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    // test exchange rate after slashing.
    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "0.9");
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    //successful call
    let msg = InitMsg {
        epoch_time: 30,
        underlying_coin_denom: "uluna".to_string(),
        undelegated_epoch: 2,
        peg_recovery_fee: Decimal::zero(),
        er_threshold: Decimal::one(),
        reward_denom: "uusd".to_string(),
    };

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);

    let owner = HumanAddr::from("owner1");
    let env = mock_env(owner, &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let reward_in = default_reward(HumanAddr::from(MOCK_CONTRACT_ADDR), "uusd".to_string());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(
        HumanAddr::from(MOCK_CONTRACT_ADDR),
        HumanAddr::from(MOCK_CONTRACT_ADDR),
    );
    token_init(&mut deps, gov_env, token_int).unwrap();

    let other_contract = HumanAddr::from("other_contract");
    let register_msg = HandleMsg::RegisterSubcontracts { contract: Reward };
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(1, exec.messages.len());

    let token_contract = HumanAddr::from("token_contract");
    let register_msg = HandleMsg::RegisterSubcontracts { contract: Token };
    let register_env = mock_env(&token_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(0, exec.messages.len());

    //check token_info
    let token_inf = TokenInfo {};
    let query_result = token_query(&deps, token_inf).unwrap();
    let value: TokenInfoResponse = from_binary(&query_result).unwrap();
    assert_eq!("bluna".to_string(), value.name);
    assert_eq!("BLUNA".to_string(), value.symbol);
    assert_eq!(Uint128::zero(), value.total_supply);
    assert_eq!(6, value.decimals);
}

#[test]
fn proper_register_validator() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract);

    // send by invalid user
    let owner = HumanAddr::from("invalid");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Only the creator can send this message")
    );

    //invalid validator
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: HumanAddr::from("invalid validator"),
    };

    let res = handle(&mut deps, owner_env, msg);
    assert_eq!(res.unwrap_err(), StdError::generic_err("Invalid validator"));

    // successful execution
    let owner = HumanAddr::from("owner1");

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let query_validatator = WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(&deps, query_validatator).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(0).unwrap(), &validator.address);

    // another validator
    do_register_validator(&mut deps, validator2.clone());

    let query_validatator2 = WhitelistedValidators {};
    let query_res: WhitelistedValidatorsResponse =
        from_binary(&query(&deps, query_validatator2).unwrap()).unwrap();
    assert_eq!(query_res.validators.get(1).unwrap(), &validator2.address);
}

#[test]
fn proper_bond() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bond_msg = HandleMsg::Bond {
        validator: validator.address,
    };

    let env = mock_env(&addr1, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let mint = &res.messages[0];
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
                    amount: Uint128(10)
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", mint),
    }

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    //get total bonded
    let bonded = TotalBonded {};
    let query_bonded: TotalBondedResponse = from_binary(&query(&deps, bonded).unwrap()).unwrap();
    assert_eq!(query_bonded.total_bonded, Uint128(10));

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

    //test no-send funds
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
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "ukrt")]);
    let res = handle(&mut deps, env.clone(), failed_bond);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("No uluna tokens sent")
    );

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };
    let address = env.contract.address;
    let gov_env = mock_env(address, &[]);

    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    set_delegation(&mut deps.querier, validator, 10, "uluna");

    //check the balance of the bob
    let balance_msg = Balance { address: bob };
    let query_result = token_query(&deps, balance_msg).unwrap();
    let value: BalanceResponse = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(10), value.balance);
}

#[test]
fn proper_deregister() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // register_validator2
    do_register_validator(&mut deps, validator2.clone());

    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

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
            assert_eq!(amount, &coin(10, "uluna"));
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

    //check invalid sender
    let msg = HandleMsg::DeregisterValidator {
        validator: validator.address,
    };

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, msg);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Only the creator can send this message",)
    );
}

#[test]
pub fn proper_update_global_index() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract.clone(), token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator.clone());

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    let token_mint = Mint {
        recipient: addr1.clone(),
        amount: Uint128(10),
    };
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

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
                    prev_balance: Uint128::zero()
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", update_g_index),
    }
}

//this will test update_global_index when there is more than one validator
#[test]
pub fn proper_update_global_index2() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract);

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

    let token_mint = Mint {
        recipient: addr1.clone(),
        amount: Uint128(10),
    };
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

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
    init_all(&mut deps, owner, reward_contract, token_contract.clone());

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    // bond to the second validator
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

    // Null message
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(0),
        msg: None,
    });

    let token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env, receive);
    assert_eq!(res.unwrap_err(), StdError::generic_err("Invalid request"));

    // unauthorized
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: Uint128(0),
        msg: Some(to_binary(&failed_unbond).unwrap()),
    });

    let invalid_env = mock_env(&invalid, &[]);
    let res = handle(&mut deps, invalid_env, receive);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // trigger handle_unbond
    let successful_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: addr1,
        amount: Uint128(0),
        msg: Some(to_binary(&successful_unbond).unwrap()),
    });

    let valid_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, valid_env, receive).unwrap_err();
    assert_eq!(res, StdError::generic_err("Invalid zero amount"));
}

#[test]
pub fn proper_unbond() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(&mut deps, owner, reward_contract, token_contract.clone());

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier.with_token_balances(&[(
        &HumanAddr::from("token"),
        &[
            (&bob, &Uint128(10u128)),
            (&HumanAddr::from("governance"), &Uint128(0)),
        ],
    )]);

    let res = handle(&mut deps, env, bond).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(10, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(10),
    };
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env.clone(), token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());
    set_delegation(&mut deps.querier, validator.clone(), 10, "uluna");

    let env = mock_env(&bob, &[]);
    let res = handle_unbond(&mut deps, env, Uint128(1), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    //read the undelegated waitlist of the current epoch for the user bob
    let waitlist = read_undelegated_wait_list(&deps.storage, 0, bob.clone()).unwrap();
    assert_eq!(Uint128(1), waitlist);

    //invalid zero
    let failed_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(0),
        msg: Some(to_binary(&failed_unbond).unwrap()),
    });
    let token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env, receive);
    assert_eq!(
        res.unwrap_err(),
        StdError::generic_err("Invalid zero amount")
    );

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

    let waitlist2 = read_undelegated_wait_list(&deps.storage, 0, bob.clone()).unwrap();
    assert_eq!(Uint128(6), waitlist2);

    //get total bonded
    let bonded = TotalBonded {};
    let query_bonded: TotalBondedResponse = from_binary(&query(&deps, bonded).unwrap()).unwrap();
    assert_eq!(query_bonded.total_bonded, Uint128(4));

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

    //pushing time forward to check the unbond message
    token_env.block.time += 31;

    let successful_bond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: bob.clone(),
        amount: Uint128(2),
        msg: Some(to_binary(&successful_bond).unwrap()),
    });
    let res = handle(&mut deps, token_env, receive.clone()).unwrap();
    assert_eq!(2, res.messages.len());

    //making sure the sent message (2nd) is undelegate
    let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
        validator: validator.address,
        amount: coin(8, "uluna"),
    });
    assert_eq!(res.messages[1], msgs);

    //making sure the epoch has passed
    let epoch = epoch_read(&deps.storage).load().unwrap();
    assert_eq!(epoch.epoch_id, 1);

    //the last request (2) gets combined and processed with the previous requests (1, 5)
    let waitlist = read_undelegated_wait_list(&deps.storage, 0, bob.clone()).unwrap();
    assert_eq!(Uint128(8), waitlist);

    let total_amount = read_total_amount(&deps.storage, 1).unwrap();
    assert_eq!(total_amount, Uint128(0));

    let burn = Burn { amount: Uint128(5) };
    let underflow_error = token_handle(&mut deps, gov_env, burn.clone());
    assert_eq!(
        underflow_error.unwrap_err(),
        StdError::generic_err("Sender does not have any cw20 tokens yet")
    );

    //mint for governance contract first
    let token_mint = Mint {
        recipient: HumanAddr::from(MOCK_CONTRACT_ADDR),
        amount: Uint128(10),
    };

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env.clone(), token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());

    let send = Send {
        contract: gov_env.message.sender.clone(),
        amount: Uint128(5),
        msg: Some(to_binary(&receive).unwrap()),
    };

    let env = mock_env(&bob, &[]);
    let send_res = token_handle(&mut deps, env, send).unwrap();
    assert_eq!(send_res.messages.len(), 3);

    let balance = Balance {
        address: gov_env.message.sender.clone(),
    };
    let query_balance: BalanceResponse =
        from_binary(&token_query(&deps, balance).unwrap()).unwrap();
    assert_eq!(query_balance.balance, Uint128(15));

    let burn_res = token_handle(&mut deps, gov_env.clone(), burn).unwrap();
    let message = &burn_res.messages[0];

    match message {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr.0, "reward");
            assert_eq!(
                msg,
                &to_binary(&DecreaseBalance {
                    address: HumanAddr::from(MOCK_CONTRACT_ADDR),
                    amount: Uint128(5),
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", message),
    }

    let balance = Balance {
        address: gov_env.message.sender,
    };
    let query_balance: BalanceResponse =
        from_binary(&token_query(&deps, balance).unwrap()).unwrap();
    assert_eq!(query_balance.balance, Uint128(10));

    let balance = Balance { address: bob };
    let query_balance: BalanceResponse =
        from_binary(&token_query(&deps, balance).unwrap()).unwrap();
    assert_eq!(query_balance.balance, Uint128(5));
}

#[test]
pub fn proper_pick_validator() {
    let mut deps = dependencies(20, &[]);

    let addr1 = HumanAddr::from("addr1000");
    let addr2 = HumanAddr::from("addr2000");
    let addr3 = HumanAddr::from("addr3000");

    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    let validator3 = sample_validator(DEFAULT_VALIDATOR3);

    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract.clone());

    do_register_validator(&mut deps, validator.clone());
    do_register_validator(&mut deps, validator2.clone());
    do_register_validator(&mut deps, validator3.clone());

    // bond to a validator
    do_bond(&mut deps, addr1.clone(), Uint128(10), validator.clone());
    do_bond(&mut deps, addr2.clone(), Uint128(300), validator2.clone());
    do_bond(&mut deps, addr3.clone(), Uint128(200), validator3.clone());

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

    //send the first burn
    let mut token_env = mock_env(&token_contract, &[]);
    let res = do_unbond(&mut deps, addr2.clone(), token_env.clone(), Uint128(50));
    assert_eq!(res.messages.len(), 1);

    token_env.block.time += 40;

    // send the second burn
    let res = do_unbond(&mut deps, addr2, token_env, Uint128(100));
    assert!(res.messages.len() >= 2);

    //check if the undelegate message is send two more than one validator.
    if res.messages.len() > 2 {
        match &res.messages[1] {
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

        match &res.messages[2] {
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
    }
}

#[test]
pub fn proper_slashing() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let addr1 = HumanAddr::from("addr1000");

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(&mut deps, owner, reward_contract, token_contract.clone());

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

    //read all the delegations stored and check them with the amount after slashing
    let all_delegations = get_all_delegations(&deps.storage).load().unwrap();
    assert_eq!(all_delegations, Uint128(900));

    let bonded = get_bonded(&deps.storage).load().unwrap();
    assert_eq!(Uint128(0), bonded);

    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "0.9");

    //bond again to see the final result
    let second_bond = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&addr1, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), second_bond).unwrap();
    assert_eq!(2, res.messages.len());

    let message = &res.messages[0];
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

    //check withdrawUnbonded message
    let withdraw_unbond_msg = HandleMsg::WithdrawUnbonded {};

    set_delegation(&mut deps.querier, validator.clone(), 1900, "uluna");
    let mut env = mock_env(&addr1, &[]);
    let _res = handle_unbond(&mut deps, env.clone(), Uint128(1000), addr1.clone()).unwrap();
    set_delegation(&mut deps.querier, validator, 1000, "uluna");

    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "0.9");

    env.block.time += 90;
    let wdraw_unbonded_res = handle(&mut deps, env, withdraw_unbond_msg).unwrap();
    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "0.9");

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

#[test]
pub fn proper_withdraw_unbonded() {
    let mut deps = dependencies(20, &[]);

    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract);

    // register_validator
    do_register_validator(&mut deps, validator.clone());

    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    let env = mock_env(&bob, &[coin(100, "uluna")]);

    //set bob's balance to 10 in token contract
    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let delegate = &res.messages[1];
    match delegate {
        CosmosMsg::Staking(StakingMsg::Delegate { validator, amount }) => {
            assert_eq!(validator.as_str(), DEFAULT_VALIDATOR);
            assert_eq!(amount, &coin(100, "uluna"));
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }

    let token_mint = Mint {
        recipient: bob.clone(),
        amount: Uint128(100),
    };

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(1, token_res.messages.len());
    set_delegation(&mut deps.querier, validator, 100, "uluna");

    let res = handle_unbond(&mut deps, env, Uint128(10), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let wdraw_unbonded_msg = HandleMsg::WithdrawUnbonded {};

    let mut env = mock_env(&bob, &[]);
    //set the block time 30 seconds from now.
    env.block.time += 30;
    let wdraw_unbonded_res = handle(&mut deps, env.clone(), wdraw_unbonded_msg.clone());

    assert_eq!(true, wdraw_unbonded_res.is_err());
    assert_eq!(
        wdraw_unbonded_res.unwrap_err(),
        StdError::generic_err("Previously requested amount is not ready yet")
    );

    //this query should be zero since the undelegated period is not passed
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));

    env.block.time += 90;

    //first query AllUnbondedRequests
    let all_unbonded = UnbondRequests {
        address: bob.clone(),
    };
    let query_unbonded = query(&deps, all_unbonded).unwrap();
    let res: UnbondRequestsResponse = from_binary(&query_unbonded).unwrap();
    assert_eq!(res.unbond_requests.len(), 1);
    //the amount should be 10
    assert_eq!(res.unbond_requests[0], Uint128(10));

    //first query AllUnbondedEpochs
    let all_user_epochs = UnbondEpochs {
        address: bob.clone(),
    };

    let query_epochs = query(&deps, all_user_epochs).unwrap();
    let res: UnbondEpochsResponse = from_binary(&query_epochs).unwrap();
    assert_eq!(res.unbond_epochs.len(), 1);
    //the epoch should be zero
    assert_eq!(res.unbond_epochs[0], 0);

    //check with query
    let withdrawable = WithdrawableUnbonded {
        address: bob.clone(),
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(10));

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
            assert_eq!(amount[0].amount, Uint128(10))
        }

        _ => panic!("Unexpected message: {:?}", sent_message),
    }

    //it should be removed
    let withdrawable = WithdrawableUnbonded {
        address: bob,
        block_time: env.block.time,
    };
    let query_with = query(&deps, withdrawable).unwrap();
    let res: WithdrawableUnbondedResponse = from_binary(&query_with).unwrap();
    assert_eq!(res.withdrawable, Uint128(0));
}

#[test]
pub fn test_update_params() {
    let mut deps = dependencies(20, &[]);
    //test with no swap denom.
    let update_prams = UpdateParams {
        epoch_time: Some(20),
        underlying_coin_denom: None,
        undelegated_epoch: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract.clone(), token_contract);

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, update_prams.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let params: Parameters = from_binary(&query(&deps, Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_time, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.undelegated_epoch, 2);
    assert_eq!(params.peg_recovery_fee, Decimal::zero());
    assert_eq!(params.er_threshold, Decimal::one());
    assert_eq!(params.reward_denom, "uusd");

    //test with some swap_denom.
    let update_prams = UpdateParams {
        epoch_time: None,
        underlying_coin_denom: None,
        undelegated_epoch: Some(3),
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
            msg: to_binary(&RewardUpdateParams {
                reward_denom: Some("ukrw".to_string()),
            })
            .unwrap()
        })]
    );

    let params: Parameters = from_binary(&query(&deps, Params {}).unwrap()).unwrap();
    assert_eq!(params.epoch_time, 20);
    assert_eq!(params.underlying_coin_denom, "uluna");
    assert_eq!(params.undelegated_epoch, 3);
    assert_eq!(params.peg_recovery_fee, Decimal::one());
    assert_eq!(params.er_threshold, Decimal::zero());
    assert_eq!(params.reward_denom, "ukrw");
}

#[test]
pub fn test_deactivate() {
    let mut deps = dependencies(20, &[]);
    let deactivate = DeactivateMsg {
        msg: Deactivated::Slashing,
    };

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract);

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, deactivate.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, deactivate).unwrap();
    assert_eq!(res.messages.len(), 0);

    //should not be able to run slashing
    let report_slashing = CheckSlashing {};
    let creator_env = mock_env(HumanAddr::from("addr1000"), &[]);
    let res = handle(&mut deps, creator_env, report_slashing);
    assert_eq!(
        res.unwrap_err(),
        (StdError::generic_err("this message is temporarily deactivated",))
    );

    let deactivate_unbond = DeactivateMsg {
        msg: Deactivated::Unbond,
    };

    let invalid_env = mock_env(HumanAddr::from("invalid"), &[]);
    let res = handle(&mut deps, invalid_env, deactivate_unbond.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());
    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, deactivate_unbond).unwrap();
    assert_eq!(res.messages.len(), 0);

    //should not be able to run slashing
    let sender = HumanAddr::from("addr1000");
    let creator_env = mock_env(&sender, &[]);
    let res = handle_unbond(&mut deps, creator_env, Uint128(10), sender);
    assert_eq!(
        res.unwrap_err(),
        (StdError::generic_err("this message is temporarily deactivated",))
    );
}

#[test]
pub fn proper_recovery_fee() {
    let mut deps = dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator_mock(&mut deps.querier);

    let update_prams = UpdateParams {
        epoch_time: None,
        underlying_coin_denom: None,
        undelegated_epoch: None,
        peg_recovery_fee: Some(Decimal::from_ratio(Uint128(1), Uint128(1000))),
        er_threshold: Some(Decimal::from_ratio(Uint128(99), Uint128(100))),
        reward_denom: None,
    };
    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner, reward_contract, token_contract.clone());

    let creator_env = mock_env(HumanAddr::from("owner1"), &[]);
    let res = handle(&mut deps, creator_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    let get_params = Params {};
    let parmas: Parameters = from_binary(&query(&deps, get_params).unwrap()).unwrap();
    assert_eq!(parmas.epoch_time, 30);
    assert_eq!(parmas.underlying_coin_denom, "uluna");
    assert_eq!(parmas.undelegated_epoch, 2);
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
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(1000u128))])]);

    let env = mock_env(&bob, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env.clone(), bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    set_delegation(&mut deps.querier, validator.clone(), 900, "uluna");

    let report_slashing = CheckSlashing {};
    let res = handle(&mut deps, env, report_slashing).unwrap();
    assert_eq!(0, res.messages.len());

    let ex_rate = ExchangeRate {};
    let query_exchange_rate: ExchangeRateResponse =
        from_binary(&query(&deps, ex_rate).unwrap()).unwrap();
    assert_eq!(query_exchange_rate.rate.to_string(), "0.9");

    //Bond again to see the applied result
    let bob = HumanAddr::from("bob");
    let bond_msg = HandleMsg::Bond {
        validator: validator.address.clone(),
    };

    deps.querier
        .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(1000u128))])]);

    let env = mock_env(&bob, &[coin(1000, "uluna")]);

    let res = handle(&mut deps, env, bond_msg).unwrap();
    assert_eq!(2, res.messages.len());

    let mint_msg = &res.messages[0];
    match mint_msg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: _,
            msg,
            send: _,
        }) => assert_eq!(
            msg,
            &to_binary(&Mint {
                recipient: bob,
                amount: Uint128(1109)
            })
            .unwrap()
        ),
        _ => panic!("Unexpected message: {:?}", mint_msg),
    }

    // check unbond message
    let unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract.clone(),
        amount: Uint128(100),
        msg: Some(to_binary(&unbond).unwrap()),
    });
    let mut token_env = mock_env(&token_contract, &[]);
    let res = handle(&mut deps, token_env.clone(), receive).unwrap();
    assert_eq!(1, res.messages.len());

    token_env.block.time += 60;

    let second_unbond = Unbond {};
    let receive = Receive(Cw20ReceiveMsg {
        sender: token_contract,
        amount: Uint128(100),
        msg: Some(to_binary(&second_unbond).unwrap()),
    });
    let res = handle(&mut deps, token_env, receive).unwrap();
    assert_eq!(2, res.messages.len());

    let undelegate_message = &res.messages[1];
    match undelegate_message {
        CosmosMsg::Staking(StakingMsg::Undelegate {
            validator: val,
            amount,
        }) => {
            assert_eq!(&validator.address, val);
            assert_eq!(amount.amount, Uint128(178));
        }
        _ => panic!("Unexpected message: {:?}", mint_msg),
    }

    let total_bonded = TotalBonded {};
    let total_bonded_query = query(&deps, total_bonded).unwrap();
    let res: TotalBondedResponse = from_binary(&total_bonded_query).unwrap();
    assert_eq!(res.total_bonded, Uint128(722));
}

#[test]
pub fn proper_update_config() {
    let mut deps = dependencies(20, &[]);

    let owner = HumanAddr::from("owner1");
    let new_onwer = HumanAddr::from("new_owner");
    let invalid_owner = HumanAddr::from("invalid_owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    // only the owner can call this message
    let update_config = UpdateConfig {
        owner: new_onwer.clone(),
    };
    let env = mock_env(&invalid_owner, &[]);
    let res = handle(&mut deps, env, update_config);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    // change the owner
    let update_config = UpdateConfig {
        owner: new_onwer.clone(),
    };
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, update_config).unwrap();
    assert_eq!(res.messages.len(), 0);

    let config = config_read(&deps.storage).load().unwrap();
    let new_owner_raw = deps.api.canonical_address(&new_onwer).unwrap();
    assert_eq!(new_owner_raw, config.creator);

    // new owner can send the owner related messages
    let update_prams = UpdateParams {
        epoch_time: None,
        underlying_coin_denom: None,
        undelegated_epoch: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };

    let new_owner_env = mock_env(&new_onwer, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams).unwrap();
    assert_eq!(res.messages.len(), 0);

    //previous owner cannot send this message
    let update_prams = UpdateParams {
        epoch_time: None,
        underlying_coin_denom: None,
        undelegated_epoch: None,
        peg_recovery_fee: None,
        er_threshold: None,
        reward_denom: None,
    };

    let new_owner_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, new_owner_env, update_prams);
    assert_eq!(res.unwrap_err(), StdError::unauthorized());
}

pub fn set_pool_info<S: Storage>(
    storage: &mut S,
    ex_rate: Decimal,
    total_boned: Uint128,
    reward_account: CanonicalAddr,
    token_account: CanonicalAddr,
) -> StdResult<()> {
    Singleton::new(storage, POOL_INFO).save(&PoolInfo {
        exchange_rate: ex_rate,
        total_bond_amount: total_boned,
        last_index_modification: 0,
        reward_account,
        is_reward_exist: true,
        is_token_exist: true,
        token_account,
    })
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
