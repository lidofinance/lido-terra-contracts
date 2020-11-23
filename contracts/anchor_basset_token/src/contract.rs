use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr,
    InitResponse, Querier, QueryRequest, StdError, StdResult, Storage, Uint128, WasmMsg, WasmQuery,
};
use cw20::{BalanceResponse, Cw20CoinHuman, Cw20ReceiveMsg, MinterResponse, TokenInfoResponse};

use crate::allowances::{
    handle_burn_from, handle_decrease_allowance, handle_increase_allowance, handle_send_from,
    handle_transfer_from, query_allowance,
};
use crate::enumerable::{query_all_accounts, query_all_allowances};
use crate::msg::{HandleMsg, QueryMsg, TokenInitMsg};
use crate::state::{balances, balances_read, token_info, token_info_read, MinterData, TokenInfo};
use anchor_basset_reward::msg::HandleMsg::{ClaimReward, UpdateUserIndex};
use cosmwasm_storage::to_length_prefixed;
use gov_courier::PoolInfo;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: TokenInitMsg,
) -> StdResult<InitResponse> {
    // check valid token info
    msg.validate()?;
    // create initial accounts
    let total_supply = create_accounts(deps, &msg.initial_balances)?;

    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap"));
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.canonical_address(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
        owner: deps.api.canonical_address(&env.message.sender)?,
    };
    token_info(&mut deps.storage).save(&data)?;

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Some(init_hook) = msg.init_hook {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: init_hook.contract_addr,
            msg: init_hook.msg,
            send: vec![],
        }));
    }

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn create_accounts<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    accounts: &[Cw20CoinHuman],
) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();
    let mut store = balances(&mut deps.storage);
    for row in accounts {
        let raw_address = deps.api.canonical_address(&row.address)?;
        store.save(raw_address.as_slice(), &row.amount)?;
        total_supply += row.amount;
    }
    Ok(total_supply)
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Transfer { recipient, amount } => handle_transfer(deps, env, recipient, amount),
        HandleMsg::Burn { amount } => handle_burn(deps, env, amount),
        HandleMsg::Send {
            contract,
            amount,
            msg,
        } => handle_send(deps, env, contract, amount, msg),
        HandleMsg::Mint { recipient, amount } => handle_mint(deps, env, recipient, amount),
        HandleMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => handle_increase_allowance(deps, env, spender, amount, expires),
        HandleMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => handle_decrease_allowance(deps, env, spender, amount, expires),
        HandleMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => handle_transfer_from(deps, env, owner, recipient, amount),
        HandleMsg::BurnFrom { owner, amount } => handle_burn_from(deps, env, owner, amount),
        HandleMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => handle_send_from(deps, env, owner, contract, amount, msg),
    }
}

pub fn handle_transfer<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let messages = update_index(&deps, env.message.sender, Some(recipient.clone()))?;

    let mut accounts = balances(&mut deps.storage);
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    accounts.update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "transfer"),
            log("from", deps.api.human_address(&sender_raw)?),
            log("to", recipient),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let owner = token_info_read(&deps.storage).load()?.owner;

    if sender_raw != owner {
        return Err(StdError::unauthorized());
    }

    let messages = update_index(&deps, env.message.sender, None)?;

    // lower balance
    let mut accounts = balances(&mut deps.storage);
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    // reduce total_supply
    token_info(&mut deps.storage).update(|mut info| {
        info.total_supply = (info.total_supply - amount)?;
        Ok(info)
    })?;

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "burn"),
            log("from", deps.api.human_address(&sender_raw)?),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    recipient: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let mut config = token_info_read(&deps.storage).load()?;
    if config.mint.is_none()
        || config.mint.as_ref().unwrap().minter
            != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(StdError::generic_err("Minting cannot exceed the cap"));
        }
    }
    token_info(&mut deps.storage).save(&config)?;

    // add amount to recipient balance
    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    let balance = balances_read(&deps.storage)
        .load(rcpt_raw.as_slice())
        .unwrap_or_default();
    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    //update the index of the holder
    let mut messages: Vec<CosmosMsg> = vec![];
    let reward_address = query_reward(&deps)?;

    if balance.is_zero() {
        let holder_msg = UpdateUserIndex {
            address: recipient.clone(),
            is_send: None,
        };
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_address,
            msg: to_binary(&holder_msg)?,
            send: vec![],
        }));
    }

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "mint"),
            log("to", recipient),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: HumanAddr,
    amount: Uint128,
    msg: Option<Binary>,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let rcpt_raw = deps.api.canonical_address(&contract)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let mut messages = update_index(&deps, env.message.sender, Some(contract.clone()))?;

    // move the tokens to the contract
    let mut accounts = balances(&mut deps.storage);
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    accounts.update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    let sender = deps.api.human_address(&sender_raw)?;
    let logs = vec![
        log("action", "send"),
        log("from", &sender),
        log("to", &contract),
        log("amount", amount),
    ];

    // create a send message
    let msg = Cw20ReceiveMsg {
        sender,
        amount,
        msg,
    }
    .into_cosmos_msg(contract)?;

    messages.push(msg);

    let res = HandleResponse {
        messages,
        log: logs,
        data: None,
    };
    Ok(res)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
    }
}

pub fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<BalanceResponse> {
    let addr_raw = deps.api.canonical_address(&address)?;
    let balance = balances_read(&deps.storage)
        .may_load(addr_raw.as_slice())?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

pub fn query_token_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<TokenInfoResponse> {
    let info = token_info_read(&deps.storage).load()?;
    let res = TokenInfoResponse {
        name: info.name,
        symbol: info.symbol,
        decimals: info.decimals,
        total_supply: info.total_supply,
    };
    Ok(res)
}

pub fn query_minter<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Option<MinterResponse>> {
    let meta = token_info_read(&deps.storage).load()?;
    let minter = match meta.mint {
        Some(m) => Some(MinterResponse {
            minter: deps.api.human_address(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };
    Ok(minter)
}

pub fn query_reward<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<HumanAddr> {
    let gov_address = deps
        .api
        .human_address(&token_info_read(&deps.storage).load().unwrap().owner)
        .unwrap();
    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: gov_address,
        key: Binary::from(to_length_prefixed(b"pool_info")),
    }))?;
    let pool_info: PoolInfo = from_binary(&res)?;
    let address = deps.api.human_address(&pool_info.reward_account).unwrap();
    Ok(address)
}

pub fn update_index<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    sender: HumanAddr,
    receiver: Option<HumanAddr>,
) -> StdResult<Vec<CosmosMsg>> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let reward_address = query_reward(&deps).unwrap();

    //this will update the sender index
    let send_reward = ClaimReward {
        recipient: Some(sender),
    };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_address.clone(),
        msg: to_binary(&send_reward).unwrap(),
        send: vec![],
    }));

    if receiver.is_some() {
        let receiver_raw = deps
            .api
            .canonical_address(&receiver.clone().unwrap())
            .unwrap();
        if balances_read(&deps.storage)
            .load(receiver_raw.as_slice())
            .is_err()
        {
            return Err(StdError::generic_err("The user does not hold any token"));
        };
        let rcv_balance = balances_read(&deps.storage)
            .load(receiver_raw.as_slice())
            .unwrap();
        println!("I am here {}", rcv_balance);
        let update_rcv_index = UpdateUserIndex {
            address: receiver.expect("The receiver has been given"),
            is_send: Some(rcv_balance),
        };

        //this will update the recipient's index
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_address,
            msg: to_binary(&update_rcv_index).unwrap(),
            send: vec![],
        }));
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, from_binary, CosmosMsg, StdError, WasmMsg};

    use super::*;
    use crate::mock_querier::mock_dependencies as dependencies;
    use crate::msg::TokenInitHook;
    use gov_courier::HandleMsg::RegisterSubContracts;
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
        let token_message = to_binary(&RegisterSubContracts {
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
        let token_message = to_binary(&RegisterSubContracts {
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
        let token_message = to_binary(&RegisterSubContracts {
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
        let data = query(&deps, QueryMsg::Balance { address: addr1 }).unwrap();
        let loaded: BalanceResponse = from_binary(&data).unwrap();
        assert_eq!(loaded.balance, Uint128(200));

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
        let claim_reward = &res.messages[0];
        match claim_reward {
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr,
                msg,
                send: _,
            }) => {
                assert_eq!(contract_addr, &HumanAddr::from("reward"));
                assert_eq!(
                    msg,
                    &to_binary(&ClaimReward {
                        recipient: Some(addr1.clone())
                    })
                    .unwrap()
                )
            }
            _ => panic!("Unexpected message: {:?}", claim_reward),
        }

        let claim_reward = &res.messages[1];
        match claim_reward {
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
                        is_send: Some(Uint128(1))
                    })
                    .unwrap()
                )
            }
            _ => panic!("Unexpected message: {:?}", claim_reward),
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

        //unauthorized
        let env = mock_env(addr1.clone(), &[]);
        let msg = HandleMsg::Burn { amount: too_much };
        let res = handle(&mut deps, env, msg);
        match res.unwrap_err() {
            StdError::Unauthorized { .. } => {}
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
            msg: Some(to_binary(&Cw20HookMsg::InitBurn {}).unwrap()),
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
        let send_msg = to_binary(&Cw20HookMsg::InitBurn {}).unwrap();

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
                msg: to_binary(&ClaimReward {
                    recipient: Some(addr1.clone())
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
                    is_send: Some(Uint128(1))
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
}
