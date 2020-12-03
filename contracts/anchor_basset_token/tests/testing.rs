use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{
    coins, from_binary, log, to_binary, Api, CosmosMsg, Extern, HumanAddr, Querier, StdError,
    Storage, Uint128, WasmMsg,
};

mod common;
use anchor_basset_reward::msg::HandleMsg::UpdateUserIndex;
use anchor_basset_token::allowances::query_allowance;
use anchor_basset_token::contract::{
    handle, init, query, query_balance, query_minter, query_token_info,
};
use anchor_basset_token::msg::QueryMsg::TokenInfo;
use anchor_basset_token::msg::{HandleMsg, QueryMsg, TokenInitHook, TokenInitMsg};
use common::mock_querier::mock_dependencies as dependencies;
use cw20::{
    AllowanceResponse, BalanceResponse, Cw20ReceiveMsg, Expiration, MinterResponse,
    TokenInfoResponse,
};
use gov_courier::HandleMsg::RegisterSubcontracts;
use gov_courier::{Cw20HookMsg, Registration};

const CANONICAL_LENGTH: usize = 20;

fn get_balance<S: Storage, A: Api, Q: Querier, T: Into<HumanAddr>>(
    deps: &Extern<S, A, Q>,
    address: T,
) -> Uint128 {
    query_balance(&deps, address.into()).unwrap().balance
}

// this will set up the init for other tests
fn do_init_with_minter<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    minter: &HumanAddr,
    cap: Option<Uint128>,
) -> TokenInfoResponse {
    _do_init(
        deps,
        Some(MinterResponse {
            minter: minter.into(),
            cap,
        }),
    )
}

// this will set up the init for other tests
fn do_init<S: Storage, A: Api, Q: Querier>(deps: &mut Extern<S, A, Q>) -> TokenInfoResponse {
    let owner = HumanAddr::from("governance");
    let mint = Some(MinterResponse {
        minter: owner,
        cap: None,
    });
    _do_init(deps, mint)
}

// this will set up the init for other tests
fn _do_init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    mint: Option<MinterResponse>,
) -> TokenInfoResponse {
    let owner = HumanAddr::from("governance");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let token_message = to_binary(&RegisterSubcontracts {
        contract: Registration::Token,
    })
    .unwrap();
    let init_msg = TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: mint.clone(),
        init_hook: Some(TokenInitHook {
            msg: token_message,
            contract_addr: owner.clone(),
        }),
        owner: owner_raw,
    };
    let env = mock_env(&owner, &[]);
    let res = init(deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    let meta = query_token_info(&deps).unwrap();
    assert_eq!(
        meta,
        TokenInfoResponse {
            name: "bluna".to_string(),
            symbol: "BLUNA".to_string(),
            decimals: 6,
            total_supply: Uint128::zero(),
        }
    );
    assert_eq!(query_minter(&deps).unwrap(), mint,);
    meta
}

pub fn do_mint<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    addr: HumanAddr,
    amount: Uint128,
) {
    let msg = HandleMsg::Mint {
        recipient: addr,
        amount,
    };
    let owner = HumanAddr::from("governance");
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(CANONICAL_LENGTH, &[]);
    let owner = HumanAddr::from("governance");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let token_message = to_binary(&RegisterSubcontracts {
        contract: Registration::Token,
    })
    .unwrap();
    let init_msg = TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.clone(),
            cap: None,
        }),
        init_hook: Some(TokenInitHook {
            msg: token_message,
            contract_addr: owner.clone(),
        }),
        owner: owner_raw,
    };
    let env = mock_env(&owner, &[]);
    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    assert_eq!(
        query_token_info(&deps).unwrap(),
        TokenInfoResponse {
            name: "bluna".to_string(),
            symbol: "BLUNA".to_string(),
            decimals: 6,
            total_supply: Uint128::zero(),
        }
    );
}

#[test]
fn init_mintable() {
    let mut deps = mock_dependencies(CANONICAL_LENGTH, &[]);
    let owner = HumanAddr::from("governance");
    let owner_raw = deps.api.canonical_address(&owner).unwrap();
    let token_message = to_binary(&RegisterSubcontracts {
        contract: Registration::Token,
    })
    .unwrap();
    let init_msg = TokenInitMsg {
        name: "bluna".to_string(),
        symbol: "BLUNA".to_string(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.clone(),
            cap: None,
        }),
        init_hook: Some(TokenInitHook {
            msg: token_message,
            contract_addr: owner.clone(),
        }),
        owner: owner_raw,
    };
    let env = mock_env(&owner, &[]);
    let res = init(&mut deps, env, init_msg).unwrap();
    assert_eq!(1, res.messages.len());

    assert_eq!(
        query_token_info(&deps).unwrap(),
        TokenInfoResponse {
            name: "bluna".to_string(),
            symbol: "BLUNA".to_string(),
            decimals: 6,
            total_supply: Uint128::zero(),
        }
    );
    assert_eq!(
        query_minter(&deps).unwrap(),
        Some(MinterResponse {
            minter: owner,
            cap: None
        }),
    );
}

#[test]
fn others_cannot_mint() {
    let mut deps = mock_dependencies(CANONICAL_LENGTH, &[]);
    do_init_with_minter(&mut deps, &HumanAddr::from("governance"), None);

    let msg = HandleMsg::Mint {
        recipient: HumanAddr::from("invalid"),
        amount: Uint128(222),
    };
    let env = mock_env(&HumanAddr::from("anyone else"), &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Unauthorized { .. } => {}
        e => panic!("expected unauthorized error, got {}", e),
    }
}

#[test]
fn no_one_mints_if_minter_unset() {
    let mut deps = mock_dependencies(CANONICAL_LENGTH, &[]);
    do_init(&mut deps);

    let msg = HandleMsg::Mint {
        recipient: HumanAddr::from("lucky"),
        amount: Uint128(222),
    };
    let env = mock_env(&HumanAddr::from("genesis"), &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Unauthorized { .. } => {}
        e => panic!("expected unauthorized error, got {}", e),
    }
}

#[test]
fn queries_work() {
    let mut deps = dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");

    let expected = do_init(&mut deps);

    // check meta query
    let loaded = query_token_info(&deps).unwrap();
    assert_eq!(expected, loaded);

    let msg = HandleMsg::Mint {
        recipient: addr1.clone(),
        amount: Uint128(200),
    };
    let owner = HumanAddr::from("governance");
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());

    // check balance query (full)
    let data = query(
        &deps,
        QueryMsg::Balance {
            address: addr1.clone(),
        },
    )
    .unwrap();
    let loaded: BalanceResponse = from_binary(&data).unwrap();
    assert_eq!(loaded.balance, Uint128(200));

    let msg = HandleMsg::Mint {
        recipient: addr1.clone(),
        amount: Uint128(200),
    };
    let owner = HumanAddr::from("governance");
    let env = mock_env(&owner, &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(1, res.messages.len());

    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: addr1.clone(),
                previous_balance: Some(Uint128(200))
            })
            .unwrap(),
            send: vec![]
        })
    );

    // check balance query (full)
    let data = query(&deps, QueryMsg::Balance { address: addr1 }).unwrap();
    let loaded: BalanceResponse = from_binary(&data).unwrap();
    assert_eq!(loaded.balance, Uint128(400));

    let token = TokenInfo {};
    let data = query(&deps, token).unwrap();
    let token: TokenInfoResponse = from_binary(&data).unwrap();
    assert_eq!(token.total_supply, Uint128(400));

    // check balance query (empty)
    let data = query(
        &deps,
        QueryMsg::Balance {
            address: HumanAddr::from("addr0002"),
        },
    )
    .unwrap();
    let loaded: BalanceResponse = from_binary(&data).unwrap();
    assert_eq!(loaded.balance, Uint128::zero());
}

#[test]
fn transfer() {
    let mut deps = dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let addr2 = HumanAddr::from("addr0002");
    let amount1 = Uint128::from(12340000u128);
    let transfer = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);

    do_init(&mut deps);

    // cannot transfer nothing
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Transfer {
        recipient: addr2.clone(),
        amount: Uint128::zero(),
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!("Invalid zero amount", msg),
        e => panic!("Unexpected error: {}", e),
    }

    //mint first
    do_mint(&mut deps, addr1.clone(), amount1);
    do_mint(&mut deps, addr2.clone(), Uint128(1));

    //cannot send
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Transfer {
        recipient: addr2.clone(),
        amount: too_much,
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }

    // cannot send from empty account
    let env = mock_env(addr2.clone(), &[]);
    let msg = HandleMsg::Transfer {
        recipient: HumanAddr::from("addr3"),
        amount: transfer,
    };
    let res = handle(&mut deps, env, msg);
    assert_eq!(res.is_err(), true);
    match res.unwrap_err() {
        StdError::GenericErr { msg, backtrace: _ } => {
            assert_eq!(msg, "The user does not hold any token")
        }
        e => panic!("Unexpected error: {}", e),
    }

    // valid transfer
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Transfer {
        recipient: addr2.clone(),
        amount: transfer,
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 2);
    let update_addr1_index = &res.messages[0];
    match update_addr1_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &HumanAddr::from("reward"));
            assert_eq!(
                msg,
                &to_binary(&UpdateUserIndex {
                    address: addr1.clone(),
                    previous_balance: Some(amount1)
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", update_addr1_index),
    }

    let updat_user_index = &res.messages[1];
    match updat_user_index {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr,
            msg,
            send: _,
        }) => {
            assert_eq!(contract_addr, &HumanAddr::from("reward"));
            assert_eq!(
                msg,
                &to_binary(&UpdateUserIndex {
                    address: addr2.clone(),
                    previous_balance: Some(Uint128(1))
                })
                .unwrap()
            )
        }
        _ => panic!("Unexpected message: {:?}", updat_user_index),
    }

    let remainder = (amount1 - transfer).unwrap();
    assert_eq!(get_balance(&deps, &addr1), remainder);
    assert_eq!(get_balance(&deps, &addr2), transfer + Uint128(1));
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        amount1 + Uint128(1)
    );
}

#[test]
fn burn() {
    let mut deps = dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let amount1 = Uint128::from(12340000u128);
    let burn = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);

    do_init(&mut deps);

    // cannot burn nothing
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Burn {
        amount: Uint128::zero(),
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!("Invalid zero amount", msg),
        e => panic!("Unexpected error: {}", e),
    }
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        Uint128::zero()
    );

    //mint first
    do_mint(&mut deps, addr1.clone(), amount1);
    do_mint(&mut deps, HumanAddr::from("governance"), Uint128(1));

    //underflow
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Burn { amount: too_much };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        amount1 + Uint128(1)
    );

    // cannot burn more than we have
    let env = mock_env(&HumanAddr::from("governance"), &[]);
    let msg = HandleMsg::Burn { amount: too_much };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        amount1 + Uint128(1)
    );

    //send should be triggered before
    let msg = HandleMsg::Send {
        contract: HumanAddr::from("governance"),
        amount: burn,
        msg: Some(to_binary(&Cw20HookMsg::Unbond {}).unwrap()),
    };
    let env = mock_env(addr1.clone(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(
        get_balance(&deps, &HumanAddr::from("governance")),
        burn + Uint128(1)
    );

    // valid burn reduces total supply
    let env = mock_env(&HumanAddr::from("governance"), &[]);
    let msg = HandleMsg::Burn { amount: burn };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 1);

    let remainder = (amount1 - burn).unwrap();
    assert_eq!(get_balance(&deps, &addr1), remainder);
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        remainder + Uint128(1)
    );
}

#[test]
fn send() {
    let mut deps = dependencies(20, &coins(2, "token"));
    let addr1 = HumanAddr::from("addr0001");
    let contract = HumanAddr::from("governance");
    let amount1 = Uint128::from(12340000u128);
    let transfer = Uint128::from(76543u128);
    let too_much = Uint128::from(12340321u128);
    let send_msg = to_binary(&Cw20HookMsg::Unbond {}).unwrap();

    do_init(&mut deps);

    // cannot send nothing
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Send {
        contract: contract.clone(),
        amount: Uint128::zero(),
        msg: Some(send_msg.clone()),
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!("Invalid zero amount", msg),
        e => panic!("Unexpected error: {}", e),
    }

    //mint first
    do_mint(&mut deps, addr1.clone(), amount1);
    do_mint(&mut deps, HumanAddr::from("governance"), Uint128(1));

    // cannot send more than we have
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Send {
        contract: contract.clone(),
        amount: too_much,
        msg: Some(send_msg.clone()),
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }

    // valid transfer
    let env = mock_env(addr1.clone(), &[]);
    let msg = HandleMsg::Send {
        contract: contract.clone(),
        amount: transfer,
        msg: Some(send_msg.clone()),
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 3);

    // ensure proper send message sent
    // this is the message we want delivered to the other side
    let binary_msg = Cw20ReceiveMsg {
        sender: addr1.clone(),
        amount: transfer,
        msg: Some(send_msg),
    }
    .into_binary()
    .unwrap();
    // and this is how it must be wrapped for the vm to process it
    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: addr1.clone(),
                previous_balance: Some(amount1)
            })
            .unwrap(),
            send: vec![]
        })
    );

    assert_eq!(
        res.messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: HumanAddr::from("governance"),
                previous_balance: Some(Uint128(1))
            })
            .unwrap(),
            send: vec![]
        })
    );
    assert_eq!(
        res.messages[2],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract.clone(),
            msg: binary_msg,
            send: vec![],
        })
    );

    // ensure balance is properly transfered
    let remainder = (amount1 - transfer).unwrap();
    assert_eq!(get_balance(&deps, &addr1), remainder);
    assert_eq!(get_balance(&deps, &contract), transfer + Uint128(1));
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        amount1 + Uint128(1)
    );
}

#[test]
fn increase_decrease_allowances() {
    let mut deps = dependencies(20, &coins(2, "token"));

    let owner = HumanAddr::from("addr0001");
    let spender = HumanAddr::from("addr0002");
    let env = mock_env(owner.clone(), &[]);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), Uint128(12340000));
    do_mint(&mut deps, spender.clone(), Uint128(12340000));
    // no allowance to start
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    assert_eq!(allowance, AllowanceResponse::default());

    // set allowance with height expiration
    let allow1 = Uint128(7777);
    let expires = Expiration::AtHeight(5432);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: allow1,
        expires: Some(expires),
    };
    handle(&mut deps, env.clone(), msg).unwrap();

    // ensure it looks good
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    assert_eq!(
        allowance,
        AllowanceResponse {
            allowance: allow1,
            expires
        }
    );

    // decrease it a bit with no expire set - stays the same
    let lower = Uint128(4444);
    let allow2 = (allow1 - lower).unwrap();
    let msg = HandleMsg::DecreaseAllowance {
        spender: spender.clone(),
        amount: lower,
        expires: None,
    };
    handle(&mut deps, env.clone(), msg).unwrap();
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    assert_eq!(
        allowance,
        AllowanceResponse {
            allowance: allow2,
            expires
        }
    );

    // increase it some more and override the expires
    let raise = Uint128(87654);
    let allow3 = allow2 + raise;
    let new_expire = Expiration::AtTime(8888888888);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: raise,
        expires: Some(new_expire),
    };
    handle(&mut deps, env.clone(), msg).unwrap();
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    assert_eq!(
        allowance,
        AllowanceResponse {
            allowance: allow3,
            expires: new_expire
        }
    );

    // decrease it below 0
    let msg = HandleMsg::DecreaseAllowance {
        spender: spender.clone(),
        amount: Uint128(99988647623876347),
        expires: None,
    };
    handle(&mut deps, env, msg).unwrap();
    let allowance = query_allowance(&deps, owner, spender).unwrap();
    assert_eq!(allowance, AllowanceResponse::default());
}

#[test]
fn allowances_independent() {
    let mut deps = dependencies(20, &coins(2, "token"));

    let owner = HumanAddr::from("addr0001");
    let spender = HumanAddr::from("addr0002");
    let spender2 = HumanAddr::from("addr0003");
    let env = mock_env(owner.clone(), &[]);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), Uint128(12340000));
    do_mint(&mut deps, spender.clone(), Uint128(12340000));
    do_mint(&mut deps, spender2.clone(), Uint128(12340000));

    // no allowance to start
    assert_eq!(
        query_allowance(&deps, owner.clone(), spender.clone()).unwrap(),
        AllowanceResponse::default()
    );
    assert_eq!(
        query_allowance(&deps, owner.clone(), spender2.clone()).unwrap(),
        AllowanceResponse::default()
    );
    assert_eq!(
        query_allowance(&deps, spender.clone(), spender2.clone()).unwrap(),
        AllowanceResponse::default()
    );

    // set allowance with height expiration
    let allow1 = Uint128(7777);
    let expires = Expiration::AtHeight(5432);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: allow1,
        expires: Some(expires),
    };
    handle(&mut deps, env.clone(), msg).unwrap();

    // set other allowance with no expiration
    let allow2 = Uint128(87654);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender2.clone(),
        amount: allow2,
        expires: None,
    };
    handle(&mut deps, env, msg).unwrap();

    // check they are proper
    let expect_one = AllowanceResponse {
        allowance: allow1,
        expires,
    };
    let expect_two = AllowanceResponse {
        allowance: allow2,
        expires: Expiration::Never {},
    };
    assert_eq!(
        query_allowance(&deps, owner.clone(), spender.clone()).unwrap(),
        expect_one
    );
    assert_eq!(
        query_allowance(&deps, owner.clone(), spender2.clone()).unwrap(),
        expect_two
    );
    assert_eq!(
        query_allowance(&deps, spender.clone(), spender2.clone()).unwrap(),
        AllowanceResponse::default()
    );

    // also allow spender -> spender2 with no interference
    let env = mock_env(spender.clone(), &[]);
    let allow3 = Uint128(1821);
    let expires3 = Expiration::AtTime(3767626296);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender2.clone(),
        amount: allow3,
        expires: Some(expires3),
    };
    handle(&mut deps, env, msg).unwrap();
    let expect_three = AllowanceResponse {
        allowance: allow3,
        expires: expires3,
    };
    assert_eq!(
        query_allowance(&deps, owner.clone(), spender.clone()).unwrap(),
        expect_one
    );
    assert_eq!(
        query_allowance(&deps, owner, spender2.clone()).unwrap(),
        expect_two
    );
    assert_eq!(
        query_allowance(&deps, spender, spender2).unwrap(),
        expect_three
    );
}

#[test]
fn no_self_allowance() {
    let mut deps = dependencies(20, &coins(2, "token"));

    let owner = HumanAddr::from("addr0001");
    let env = mock_env(owner.clone(), &[]);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), Uint128(12340000));

    // self-allowance
    let msg = HandleMsg::IncreaseAllowance {
        spender: owner.clone(),
        amount: Uint128(7777),
        expires: None,
    };
    let res = handle(&mut deps, env.clone(), msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Cannot set allowance to own account"),
        e => panic!("Unexpected error: {}", e),
    }

    // decrease self-allowance
    let msg = HandleMsg::DecreaseAllowance {
        spender: owner,
        amount: Uint128(7777),
        expires: None,
    };
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Cannot set allowance to own account"),
        e => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn transfer_from_respects_limits() {
    let mut deps = dependencies(20, &[]);
    let owner = HumanAddr::from("addr0001");
    let spender = HumanAddr::from("addr0002");
    let rcpt = HumanAddr::from("addr0003");

    let start = Uint128(999999);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), start);
    do_mint(&mut deps, spender.clone(), start);
    do_mint(&mut deps, rcpt.clone(), Uint128(1));

    // provide an allowance
    let allow1 = Uint128(77777);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: allow1,
        expires: None,
    };
    let env = mock_env(owner.clone(), &[]);
    handle(&mut deps, env, msg).unwrap();

    // valid transfer of part of the allowance
    let transfer = Uint128(44444);
    let msg = HandleMsg::TransferFrom {
        owner: owner.clone(),
        recipient: rcpt.clone(),
        amount: transfer,
    };
    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.log[0], log("action", "transfer_from"));
    assert_eq!(res.messages.len(), 2);

    //test invoke update_index
    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: owner.clone(),
                previous_balance: Some(start)
            })
            .unwrap(),
            send: vec![]
        })
    );

    assert_eq!(
        res.messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: rcpt.clone(),
                previous_balance: Some(Uint128(1))
            })
            .unwrap(),
            send: vec![]
        })
    );

    // make sure money arrived
    assert_eq!(get_balance(&deps, &owner), (start - transfer).unwrap());
    assert_eq!(get_balance(&deps, &rcpt), transfer + Uint128(1));

    // ensure it looks good
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    let expect = AllowanceResponse {
        allowance: (allow1 - transfer).unwrap(),
        expires: Expiration::Never {},
    };
    assert_eq!(expect, allowance);

    // cannot send more than the allowance
    let msg = HandleMsg::TransferFrom {
        owner: owner.clone(),
        recipient: rcpt.clone(),
        amount: Uint128(33443),
    };
    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }

    // let us increase limit, but set the expiration (default env height is 12_345)
    let env = mock_env(owner.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: Uint128(1000),
        expires: Some(Expiration::AtHeight(env.block.height)),
    };
    handle(&mut deps, env, msg).unwrap();

    // we should now get the expiration error
    let msg = HandleMsg::TransferFrom {
        owner,
        recipient: rcpt,
        amount: Uint128(33443),
    };
    let env = mock_env(spender, &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Allowance is expired"),
        e => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn burn_from_respects_limits() {
    let mut deps = dependencies(20, &[]);
    let owner = HumanAddr::from("addr0001");
    let spender = HumanAddr::from("addr0002");

    let start = Uint128(999999);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), start);
    do_mint(&mut deps, spender.clone(), start);

    let allow1 = Uint128(77777);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: allow1,
        expires: None,
    };
    let env = mock_env(owner.clone(), &[]);
    handle(&mut deps, env, msg).unwrap();

    // valid burn of part of the allowance
    let transfer = Uint128(44444);
    let msg = HandleMsg::BurnFrom {
        owner: owner.clone(),
        amount: transfer,
    };

    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.log[0], log("action", "burn_from"));
    assert_eq!(res.messages.len(), 1);

    //test invoke update_index
    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: owner.clone(),
                previous_balance: Some(start)
            })
            .unwrap(),
            send: vec![]
        })
    );

    // make sure money burnt
    assert_eq!(get_balance(&deps, &owner), (start - transfer).unwrap());

    //total_supply is 2 * start since we issued for the spender as well
    assert_eq!(
        query_token_info(&deps).unwrap().total_supply,
        ((start.multiply_ratio(Uint128(2), Uint128(1))) - transfer).unwrap()
    );

    // ensure it looks good
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    let expect = AllowanceResponse {
        allowance: (allow1 - transfer).unwrap(),
        expires: Expiration::Never {},
    };
    assert_eq!(expect, allowance);

    // cannot burn more than the allowance
    let msg = HandleMsg::BurnFrom {
        owner: owner.clone(),
        amount: Uint128(33443),
    };

    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }

    // let us increase limit, but set the expiration (default env height is 12_345)
    let env = mock_env(owner.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: Uint128(1000),
        expires: Some(Expiration::AtHeight(env.block.height)),
    };
    handle(&mut deps, env, msg).unwrap();

    // we should now get the expiration error
    let msg = HandleMsg::BurnFrom {
        owner,
        amount: Uint128(33443),
    };
    let env = mock_env(spender, &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Allowance is expired"),
        e => panic!("Unexpected error: {}", e),
    }
}

#[test]
fn send_from_respects_limits() {
    let mut deps = dependencies(20, &[]);
    let owner = HumanAddr::from("addr0001");
    let spender = HumanAddr::from("addr0002");
    let contract = HumanAddr::from("governance");
    let send_msg = to_binary(&Some(123)).unwrap();

    let start = Uint128(999999);
    do_init(&mut deps);

    //mint first
    do_mint(&mut deps, owner.clone(), start);
    do_mint(&mut deps, spender.clone(), start);
    do_mint(&mut deps, contract.clone(), start);

    // provide an allowance
    let allow1 = Uint128(77777);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: allow1,
        expires: None,
    };
    let env = mock_env(owner.clone(), &[]);
    handle(&mut deps, env, msg).unwrap();

    // valid send of part of the allowance
    let transfer = Uint128(44444);
    let msg = HandleMsg::SendFrom {
        owner: owner.clone(),
        amount: transfer,
        contract: contract.clone(),
        msg: Some(send_msg.clone()),
    };
    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.log[0], log("action", "send_from"));
    assert_eq!(3, res.messages.len());

    //test invoke update_index
    assert_eq!(
        res.messages[0],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: owner.clone(),
                previous_balance: Some(start)
            })
            .unwrap(),
            send: vec![]
        })
    );

    assert_eq!(
        res.messages[1],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: HumanAddr::from("reward"),
            msg: to_binary(&UpdateUserIndex {
                address: HumanAddr::from("governance"),
                previous_balance: Some(start)
            })
            .unwrap(),
            send: vec![]
        })
    );

    // we record this as sent by the one who requested, not the one who was paying
    let binary_msg = Cw20ReceiveMsg {
        sender: spender.clone(),
        amount: transfer,
        msg: Some(send_msg.clone()),
    }
    .into_binary()
    .unwrap();
    assert_eq!(
        res.messages[2],
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract.clone(),
            msg: binary_msg,
            send: vec![],
        })
    );

    // make sure money sent
    assert_eq!(get_balance(&deps, &owner), (start - transfer).unwrap());
    assert_eq!(get_balance(&deps, &contract), start + transfer);

    // ensure it looks good
    let allowance = query_allowance(&deps, owner.clone(), spender.clone()).unwrap();
    let expect = AllowanceResponse {
        allowance: (allow1 - transfer).unwrap(),
        expires: Expiration::Never {},
    };
    assert_eq!(expect, allowance);

    // cannot send more than the allowance
    let msg = HandleMsg::SendFrom {
        owner: owner.clone(),
        amount: Uint128(33443),
        contract: contract.clone(),
        msg: Some(send_msg.clone()),
    };
    let env = mock_env(spender.clone(), &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::Underflow { .. } => {}
        e => panic!("Unexpected error: {}", e),
    }

    // let us increase limit, but set the expiration to current block (expired)
    let env = mock_env(owner.clone(), &[]);
    let msg = HandleMsg::IncreaseAllowance {
        spender: spender.clone(),
        amount: Uint128(1000),
        expires: Some(Expiration::AtHeight(env.block.height)),
    };
    handle(&mut deps, env, msg).unwrap();

    // we should now get the expiration error
    let msg = HandleMsg::SendFrom {
        owner,
        amount: Uint128(33443),
        contract,
        msg: Some(send_msg),
    };
    let env = mock_env(spender, &[]);
    let res = handle(&mut deps, env, msg);
    match res.unwrap_err() {
        StdError::GenericErr { msg, .. } => assert_eq!(msg, "Allowance is expired"),
        e => panic!("Unexpected error: {}", e),
    }
}
