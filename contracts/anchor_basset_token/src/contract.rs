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
use anchor_basset_reward::msg::HandleMsg::UpdateUserIndex;
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

    let holder_msg = if balance.is_zero() {
        UpdateUserIndex {
            address: recipient.clone(),
            is_send: None,
        }
    } else {
        UpdateUserIndex {
            address: recipient.clone(),
            is_send: Some(balance),
        }
    };

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_address,
        msg: to_binary(&holder_msg)?,
        send: vec![],
    }));

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

    //update the index of the sender and send the reward to pending reward.
    let sender_raw = deps.api.canonical_address(&sender).unwrap();
    let sender_balance = balances_read(&deps.storage)
        .load(sender_raw.as_slice())
        .unwrap_or_default();
    if sender_balance.is_zero() {
        return Err(StdError::generic_err(
            "Sender does not have any cw20 token yet",
        ));
    }
    if !sender_balance.is_zero() {
        let update_sender_index = UpdateUserIndex {
            address: sender,
            is_send: Some(sender_balance),
        };

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_address.clone(),
            msg: to_binary(&update_sender_index).unwrap(),
            send: vec![],
        }));
    }

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
