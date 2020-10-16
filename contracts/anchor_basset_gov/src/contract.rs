use cosmwasm_std::{
    coin, coins, log, to_binary, Api, BankMsg, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, InitResponse, Querier, StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::msg::{HandleMsg, InitMsg};
use crate::state::{
    balances, balances_read, claim_read, claim_store, pool_info, pool_info_read, token_info,
    token_info_read, token_state, token_state_read, EpocId, PoolInfo, TokenInfo, TokenState,
    Undelegation, UNDELEGATED_PERIOD,
};
use anchor_basset_reward::hook::InitHook;
use anchor_basset_reward::init::RewardInitMsg;
use std::ops::Add;

const FIRST_EPOC: u64 = 1;
const EPOC_PER_UNDELEGATION_PERIOD: u64 = UNDELEGATED_PERIOD / 86400;
// For updating GlobalIndex, since it is a costly message, we send a withdraw message every day.
//DAY is supposed to help us to check whether a day is passed from the last update GlobalIndex or not.
const DAY: u64 = 86400;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    // validate token info
    msg.validate()?;

    // store token info
    let initial_total_supply = Uint128::zero();
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: initial_total_supply,
    };
    token_info(&mut deps.storage).save(&data)?;

    let token = TokenState::default();

    token_state(&mut deps.storage).save(&token)?;

    let pool = PoolInfo::default();
    pool_info(&mut deps.storage).save(&pool)?;

    //Instantiate the other contract to help us to manage the global index calculation.
    let reward_message = to_binary(&HandleMsg::Register {})?;
    let res = InitResponse {
        messages: vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
            code_id: 0,
            msg: to_binary(&RewardInitMsg {
                owner: deps.api.canonical_address(&env.contract.address)?,
                init_hook: Some(InitHook {
                    msg: reward_message,
                    contract_addr: env.contract.address,
                }),
            })?,
            send: vec![],
            label: None,
        })],
        log: vec![],
    };
    Ok(res)
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::Mint { validator, amount } => handle_mint(deps, env, validator, amount),
        HandleMsg::ClaimRewards {} => handle_reward(deps, env),
        HandleMsg::Send { recipient, amount } => handle_send(deps, env, recipient, amount),
        HandleMsg::InitBurn { amount } => handle_burn(deps, env, amount),
        HandleMsg::FinishBurn { amount } => handle_finish(deps, env, amount),
        HandleMsg::Register {} => handle_register(deps, env),
    }
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let mut token = token_info_read(&deps.storage).load()?;

    //Check whether the account has sent the native coin in advance.
    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == token.name)
        .ok_or_else(|| StdError::generic_err(format!("No {} tokens sent", &token.name)))?;

    //update pool_info
    let mut pool = pool_info_read(&deps.storage).load()?;
    pool.total_bond_amount += amount;
    pool.total_issued += amount;

    let reward_index = pool.reward_index;

    let amount_with_exchange_rate =
        if pool.total_bond_amount.is_zero() || pool.total_issued.is_zero() {
            amount
        } else {
            pool.update_exchange_rate();
            let exchange_rate = pool.exchange_rate;
            exchange_rate * amount
        };

    pool_info(&mut deps.storage).save(&pool)?;

    // Issue the bluna token for sender
    let sender = env.message.sender;
    let rcpt_raw = deps.api.canonical_address(&sender)?;
    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount_with_exchange_rate)
    })?;

    let added_amount = payment.amount.add(amount);

    token.total_supply += amount_with_exchange_rate;

    //save token_info
    token_info(&mut deps.storage).save(&token)?;

    //update the token_status
    let mut token_status = token_state_read(&deps.storage).load()?;

    token_status
        .delegation_map
        .insert(validator.clone(), amount);

    token_status.holder_map.insert(sender.clone(), reward_index);

    token_state(&mut deps.storage).save(&token_status)?;

    // bond them to the validator
    let res = HandleResponse {
        messages: vec![StakingMsg::Delegate {
            validator,
            amount: payment.clone(),
        }
        .into()],
        log: vec![
            log("action", "mint"),
            log("from", sender.clone()),
            log("bonded", payment.amount),
            log("minted", added_amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_reward<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let sender = env.message.sender.clone();
    let rcpt_raw = deps.api.canonical_address(&sender)?;
    let contract_addr = env.contract.address.clone();

    //get the token_state
    let mut token = token_state_read(&deps.storage).load()?;

    if token.holder_map.get(&sender).is_none() {
        return Err(StdError::generic_err(
            "The sender has not requested any tokens",
        ));
    }
    let all_holders = token.holder_map.clone();
    let sender_reward_index = all_holders
        .get(&sender)
        .expect("The existence of the sender has been checked");

    let pool = pool_info_read(&deps.storage).load()?;
    let reward_addr = deps.api.human_address(&pool.reward_account)?;
    let user_balance = balances_read(&deps.storage).load(rcpt_raw.as_slice())?;

    token.holder_map.insert(sender.clone(), pool.reward_index);

    //Retrieve all the validators.
    let delegation_list = token.delegation_map.clone();
    let mut validators: Vec<HumanAddr> = Vec::new();
    let reward: Uint128;
    for (key, _) in delegation_list {
        validators.push(key);
    }
    //Withdraw all rewards from all validators.
    if withdraw_all_rewards(
        validators,
        pool.clone(),
        env.block.time,
        reward_addr.clone(),
    ) {
        update_index(deps, contract_addr.clone());
        let general_index = pool.reward_index;
        reward = calculate_reward(general_index, sender_reward_index, user_balance).unwrap();
    } else {
        let general_index = pool.reward_index;
        reward = calculate_reward(general_index, sender_reward_index, user_balance).unwrap();
    }
    token_state(&mut deps.storage).save(&token)?;

    balances(&mut deps.storage).update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + reward)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "claim_reward"),
            log("to", sender),
            log("amount", reward),
        ],
        data: None,
    };
    Ok(res)
}

// Since we cannot query validators' reward, we have to Withdraw all the rewards
// and then update the global index.
// withdraw returns a bool. if is true it means the index has changed.
// if it is false it means nothing has changed.
pub fn withdraw_all_rewards(
    validators: Vec<HumanAddr>,
    pool: PoolInfo,
    block_time: u64,
    contract_addr: HumanAddr,
) -> bool {
    if pool.current_block_time > block_time - DAY {
        for val in validators {
            let addr = contract_addr.clone();
            let msg: StakingMsg = StakingMsg::Withdraw {
                validator: val,
                recipient: Some(contract_addr.clone()),
            };
            WasmMsg::Execute {
                contract_addr: addr,
                msg: to_binary(&msg).unwrap(),
                send: vec![],
            };
        }
        return true;
    }
    false
}

// check the balance of the reward contract and calculate the global index.
pub fn update_index<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    reward_addr: HumanAddr,
) {
    let mut pool = pool_info_read(&deps.storage).load().unwrap();
    let token_info = token_info_read(&deps.storage).load().unwrap();
    let balance = deps
        .querier
        .query_balance(reward_addr, &token_info.name)
        .unwrap();
    let prev_reward_index = pool.reward_index.clone();
    let total_bonded = pool.total_bond_amount.clone();
    pool.reward_index =
        prev_reward_index + Decimal::from_ratio(balance.amount.u128(), total_bonded.u128());

    pool_info(&mut deps.storage).save(&pool).unwrap();
}

// calculate the reward based on the sender's index and the global index.
pub fn calculate_reward(
    general_index: Decimal,
    user_index: &Decimal,
    user_balance: Uint128,
) -> StdResult<Uint128> {
    general_index * user_balance - *user_index * user_balance
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
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

    let mut accounts = balances(&mut deps.storage);
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    accounts.update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "send"),
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

    let sender_human = env.message.sender.clone();
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    let mut token = token_state_read(&mut deps.storage).load()?;
    let mut accounts = balances(&mut deps.storage);

    //claim the reward.
    let msg = HandleMsg::ClaimRewards {};
    WasmMsg::Execute {
        contract_addr: env.contract.address.clone(),
        msg: to_binary(&msg)?,
        send: vec![],
    };

    // lower balance
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    // reduce total_supply
    token_info(&mut deps.storage).update(|mut info| {
        info.total_supply = (info.total_supply - amount)?;
        Ok(info)
    })?;

    //compute Epoc time
    let block_time = env.block.time;
    token.compute_current_epoc(block_time);
    let epoc = EpocId {
        epoc_id: token.current_epoc.clone(),
    };

    //check whether the epoc is passed or not. If epoc is passed send an undelegation message.
    if token.is_epoc_passed(block_time) && epoc.epoc_id > FIRST_EPOC {
        handle_undelegate(deps, env, epoc.clone(), token.clone());
    }

    if token.is_epoc_passed(block_time) {
        let mut undelegated = Undelegation::default();
        undelegated.claim += amount;
        undelegated
            .undelegated_wait_list_map
            .insert(sender_human, amount);
        token.undelegated_wait_list.insert(epoc, undelegated);
    } else {
        let mut undelegated = token.undelegated_wait_list.remove(&epoc).unwrap();
        undelegated.compute_claim();
        undelegated
            .undelegated_wait_list_map
            .insert(sender_human, amount);
        token.undelegated_wait_list.insert(epoc, undelegated);
    }
    token_state(&mut deps.storage).save(&token)?;

    pool_info(&mut deps.storage).update(|mut pool| {
        pool.total_issued = (pool.total_issued - amount)?;
        Ok(pool)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "burn"),
            log("from", deps.api.human_address(&sender_raw)?),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_undelegate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoc: EpocId,
    mut token: TokenState,
) {
    let token_inf = token_info_read(&deps.storage).load().unwrap();
    let undelegated = token.undelegated_wait_list.get(&epoc).unwrap();
    let claimed = undelegated.claim;
    let validator = token.choose_validator(claimed);
    let amount = token
        .delegation_map
        .get(&validator)
        .expect("The validator has exist");
    let new_delegation = amount.0 - &claimed.0;
    token
        .delegation_map
        .insert(validator.clone(), Uint128(new_delegation));

    let msgs: Vec<StakingMsg> = vec![StakingMsg::Undelegate {
        validator,
        amount: coin(claimed.u128(), &token_inf.name),
    }
    .into()];

    WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&msgs).unwrap(),
        send: vec![],
    };
}

pub fn handle_finish<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    let sender_human = env.message.sender.clone();
    let contract_address = env.contract.address.clone();

    let mut token = token_state_read(&mut deps.storage).load()?;

    let block_time = env.block.time;

    if !token.is_valid_address(&sender_human) {
        return Err(StdError::unauthorized());
    }

    let rcpt_raw = deps.api.canonical_address(&env.message.sender.clone())?;
    let claim_balance = claim_read(&deps.storage).load(rcpt_raw.as_slice())?;

    //The user's request might have processed before. Therefore, we need to check its claim balance.
    if amount <= claim_balance {
        return handle_send_undelegation(amount, sender_human, contract_address);
    }

    token.compute_current_epoc(block_time);
    let current_epoc_id = token.current_epoc.clone();
    // Compute all of burn requests with epoc Id corresponding to 21 (can be changed to arbitrary value) days ago
    let epoc_id = EpocId {
        epoc_id: get_before_undelegation_epoc(current_epoc_id),
    };
    for (key, value) in token.undelegated_wait_list.clone() {
        if key < epoc_id {
            for (address, undelegated_amount) in value.undelegated_wait_list_map {
                let raw_address = deps.api.canonical_address(&address)?;
                claim_store(&mut deps.storage)
                    .update(raw_address.as_slice(), |claim: Option<Uint128>| {
                        Ok(claim.unwrap_or_default() + undelegated_amount)
                    })?;
            }
            token.undelegated_wait_list.remove(&key);
        }
    }

    return handle_send_undelegation(amount, sender_human, contract_address);
}

pub fn get_before_undelegation_epoc(current_epoc: u64) -> u64 {
    current_epoc - EPOC_PER_UNDELEGATION_PERIOD
}

pub fn handle_send_undelegation(
    amount: Uint128,
    to_address: HumanAddr,
    contract_address: HumanAddr,
) -> StdResult<HandleResponse> {
    // Create Send message
    let msgs = vec![BankMsg::Send {
        from_address: contract_address.clone(),
        to_address: to_address,
        amount: coins(Uint128::u128(&amount), "uluna"),
    }
    .into()];

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "finish_burn"),
            log("from", contract_address),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_register<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut pool = pool_info_read(&deps.storage).load()?;
    let raw_sender = deps.api.canonical_address(&env.message.sender)?;
    pool.reward_account = raw_sender.clone();

    pool_info(&mut deps.storage).save(&pool)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "register"), log("sub_contract", raw_sender)],
        data: None,
    };
    Ok(res)
}
