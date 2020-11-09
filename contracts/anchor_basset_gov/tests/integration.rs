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
    coin, from_binary, Api, BankMsg, CanonicalAddr, Coin, CosmosMsg, Decimal, Extern,
    FullDelegation, HumanAddr, InitResponse, Querier, StakingMsg, StdResult, Storage, Uint128,
    Validator,
};

use cosmwasm_std::testing::{mock_dependencies, mock_env, MockQuerier};

use anchor_bluna::msg::InitMsg;
use gov_courier::{HandleMsg, PoolInfo};

use anchor_bluna::contract::{handle, handle_burn, init};

use anchor_basset_reward::contracts::init as reward_init;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::state::Config;
use anchor_basset_token::contract::{
    handle as token_handle, init as token_init, query as token_query,
};
use anchor_basset_token::msg::HandleMsg::Mint;
use anchor_basset_token::msg::QueryMsg::{Balance, TokenInfo};
use anchor_basset_token::msg::TokenInitMsg;
use anchor_basset_token::state::{MinterData, TokenInfo as TokenConfig};
use cosmwasm_storage::Singleton;
use cw20::{BalanceResponse, MinterResponse, TokenInfoResponse};
use gov_courier::Registration::{Reward, Token};

const DEFAULT_VALIDATOR: &str = "default-validator";
const DEFAULT_VALIDATOR2: &str = "default-validator2";
pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub static POOL_INFO: &[u8] = b"pool_info";
pub static CONFIG: &[u8] = b"config";
const TOKEN_INFO_KEY: &[u8] = b"token_info";

fn sample_validator<U: Into<HumanAddr>>(addr: U) -> Validator {
    Validator {
        address: addr.into(),
        commission: Decimal::percent(3),
        max_commission: Decimal::percent(10),
        max_change_rate: Decimal::percent(1),
    }
}

fn set_validator(querier: &mut MockQuerier) {
    querier.update_staking(
        "uluna",
        &[
            sample_validator(DEFAULT_VALIDATOR),
            sample_validator(DEFAULT_VALIDATOR2),
        ],
        &[],
    );
}

fn default_token(owner: CanonicalAddr, minter: HumanAddr) -> TokenInitMsg {
    TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse { minter, cap: None }),
        init_hook: None,
        owner,
    }
}

fn default_reward(owner: CanonicalAddr) -> RewardInitMsg {
    RewardInitMsg {
        owner,
        init_hook: None,
    }
}

pub fn init_all<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    owner: HumanAddr,
    reward_contract: HumanAddr,
    token_contract: HumanAddr,
) {
    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        reward_code_id: 0,
        token_code_id: 0,
    };

    let gov_address = deps
        .api
        .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
        .unwrap();

    let gov_env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
    let env = mock_env(owner.clone(), &[]);
    init(&mut deps, env, msg).unwrap();

    let reward_in = default_reward(gov_address.clone());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(gov_address.clone(), owner);
    token_init(&mut deps, gov_env, token_int).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Reward };
    let register_env = mock_env(reward_contract.clone(), &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let register_msg = HandleMsg::RegisterSubContracts { contract: Token };
    let register_env = mock_env(token_contract.clone(), &[]);
    handle(&mut deps, register_env, register_msg).unwrap();

    let reward_raw = deps.api.canonical_address(&reward_contract).unwrap();
    let token_raw = deps.api.canonical_address(&token_contract).unwrap();
    set_pool_info(
        &mut deps.storage,
        Decimal::one(),
        Uint128::zero(),
        Uint128::zero(),
        reward_raw,
        token_raw,
    )
    .unwrap();
    set_reward_config(&mut deps.storage, gov_address.clone()).unwrap();
    set_token_info(&mut deps.storage, gov_address).unwrap();
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let msg = InitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        reward_code_id: 0,
        token_code_id: 0,
    };

    let gov_address = deps
        .api
        .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
        .unwrap();
    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);

    let owner = HumanAddr::from("addr0000");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();

    let env = mock_env(owner, &[]);

    // we can just call .unwrap() to assert this was a success
    let res: InitResponse = init(&mut deps, env, msg).unwrap();
    assert_eq!(2, res.messages.len());

    let reward_in = default_reward(gov_address.clone());
    reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

    let token_int = default_token(gov_address, HumanAddr::from(MOCK_CONTRACT_ADDR));
    token_init(&mut deps, gov_env, token_int).unwrap();
    set_token_info(&mut deps.storage, owner_raw).unwrap();

    let other_contract = HumanAddr::from("other_contract");
    let register_msg = HandleMsg::RegisterSubContracts { contract: Reward };
    let register_env = mock_env(&other_contract, &[]);
    let exec = handle(&mut deps, register_env, register_msg).unwrap();
    assert_eq!(1, exec.messages.len());

    let token_contract = HumanAddr::from("token_contract");
    let register_msg = HandleMsg::RegisterSubContracts { contract: Token };
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
fn proper_mint() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
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
    let address = env.contract.address;
    let gov_env = mock_env(address, &[]);

    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    //check the balance of the bob
    let balance_msg = Balance { address: bob };
    let query_result = token_query(&deps, balance_msg).unwrap();
    let value: BalanceResponse = from_binary(&query_result).unwrap();
    assert_eq!(Uint128(10), value.balance);
}

#[test]
fn proper_deregister() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    let validator2 = sample_validator(DEFAULT_VALIDATOR2);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner1");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let owner_env = mock_env(owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    let msg = HandleMsg::RegisterValidator {
        validator: validator2.address,
    };

    let res = handle(&mut deps, owner_env.clone(), msg).unwrap();
    assert_eq!(0, res.messages.len());

    set_delegation(&mut deps.querier, 10, "uluna");

    let msg = HandleMsg::DeRegisterValidator {
        validator: validator.address,
    };

    let res = handle(&mut deps, owner_env, msg).unwrap();
    assert_eq!(2, res.messages.len());
}

#[test]
pub fn proper_claim_reward() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
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
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let reward_msg = HandleMsg::UpdateGlobalIndex {};

    let env = mock_env(&bob, &[]);
    let res = handle(&mut deps, env, reward_msg).unwrap();
    assert_eq!(3, res.messages.len());
}

#[test]
pub fn proper_init_burn() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");
    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env, mint_msg).unwrap();
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
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let env = mock_env(&bob, &[]);
    let res = handle_burn(&mut deps, env, Uint128(1), bob).unwrap();
    assert_eq!(1, res.messages.len());
}

#[test]
pub fn proper_finish() {
    let mut deps = mock_dependencies(20, &[]);
    let validator = sample_validator(DEFAULT_VALIDATOR);
    set_validator(&mut deps.querier);

    let owner = HumanAddr::from("owner");
    let token_contract = HumanAddr::from("token");
    let reward_contract = HumanAddr::from("reward");

    init_all(&mut deps, owner.clone(), reward_contract, token_contract);

    let env = mock_env(&owner, &[]);
    let msg = HandleMsg::RegisterValidator {
        validator: validator.address.clone(),
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    let bob = HumanAddr::from("bob");
    let mint_msg = HandleMsg::Mint {
        validator: validator.address,
    };

    let env = mock_env(&bob, &[coin(10, "uluna")]);

    let res = handle(&mut deps, env.clone(), mint_msg).unwrap();
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

    set_delegation(&mut deps.querier, 10, "uluna");

    let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
    let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
    assert_eq!(0, token_res.messages.len());

    let res = handle_burn(&mut deps, env, Uint128(1), bob.clone()).unwrap();
    assert_eq!(1, res.messages.len());

    let finish_msg = HandleMsg::FinishBurn { amount: Uint128(1) };

    let mut env = mock_env(&bob, &[coin(10, "uluna")]);
    //set the block time 6 hours from now.
    env.block.time += 22600;
    let finish_res = handle(&mut deps, env.clone(), finish_msg.clone()).is_err();

    assert_eq!(false, finish_res);

    env.block.time = 1573911419;
    let finish_res = handle(&mut deps, env, finish_msg).unwrap();

    assert_eq!(finish_res.messages.len(), 1);

    let delegate = &finish_res.messages[0];
    match delegate {
        CosmosMsg::Bank(BankMsg::Send {
            from_address: _,
            to_address,
            amount,
        }) => {
            assert_eq!(to_address, &bob);
            assert_eq!(amount.get(0).unwrap().amount, Uint128(1))
        }
        _ => panic!("Unexpected message: {:?}", delegate),
    }
}

pub fn set_pool_info<S: Storage>(
    storage: &mut S,
    ex_rate: Decimal,
    total_boned: Uint128,
    total_issued: Uint128,
    reward_account: CanonicalAddr,
    token_account: CanonicalAddr,
) -> StdResult<()> {
    Singleton::new(storage, POOL_INFO).save(&PoolInfo {
        exchange_rate: ex_rate,
        total_bond_amount: total_boned,
        total_issued,
        last_index_modification: 0,
        reward_account,
        is_reward_exist: true,
        is_token_exist: true,
        token_account,
    })
}

pub fn set_reward_config<S: Storage>(storage: &mut S, owner: CanonicalAddr) -> StdResult<()> {
    Singleton::new(storage, CONFIG).save(&Config { owner })
}

pub fn set_token_info<S: Storage>(storage: &mut S, owner: CanonicalAddr) -> StdResult<()> {
    Singleton::new(storage, TOKEN_INFO_KEY).save(&TokenConfig {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        total_supply: Default::default(),
        mint: Some(MinterData {
            minter: owner.clone(),
            cap: None,
        }),
        owner,
    })
}

fn set_delegation(querier: &mut MockQuerier, amount: u128, denom: &str) {
    querier.update_staking(
        "uluna",
        &[sample_validator(DEFAULT_VALIDATOR)],
        &[sample_delegation(DEFAULT_VALIDATOR, coin(amount, denom))],
    );
}

fn sample_delegation<U: Into<HumanAddr>>(addr: U, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = coin(0, &amount.denom);
    FullDelegation {
        validator: addr.into(),
        delegator: HumanAddr::from(MOCK_CONTRACT_ADDR),
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}
