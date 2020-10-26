use cosmwasm_std::{
    coin, coins, log, to_binary, Api, BankMsg, Binary, CanonicalAddr, CosmosMsg, Decimal, Env,
    Extern, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg, StdError, StdResult,
    Storage, Uint128, WasmMsg,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg, TokenInfoResponse};
use crate::state::{
    balances, balances_read, claim_read, claim_store, epoc_read, is_valid_validator, pool_info,
    pool_info_read, read_all_epocs, read_delegation_map, read_holder_map, read_holders,
    read_total_amount, read_undelegated_wait_list, read_undelegated_wait_list_for_epoc,
    read_valid_validators, read_validators, remove_white_validators, save_all_epoc, save_epoc,
    store_delegation_map, store_holder_map, store_total_amount, store_undelegated_wait_list,
    store_white_validators, token_info, token_info_read, AllEpoc, EpocId, PoolInfo, TokenInfo,
    EPOC,
};
use anchor_basset_reward::hook::InitHook;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::msg::HandleMsg::{SendReward, Swap};
use rand::Rng;
use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::ops::Add;

const EPOC_PER_UNDELEGATION_PERIOD: u64 = 83;
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
    let sender = env.message.sender;
    let sndr_raw = deps.api.canonical_address(&sender)?;
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply: initial_total_supply,
        creator: sndr_raw,
    };
    token_info(&mut deps.storage).save(&data)?;

    let pool = PoolInfo {
        exchange_rate: Decimal::one(),
        ..Default::default()
    };
    pool_info(&mut deps.storage).save(&pool)?;

    //store the first epoc.
    let first_epoc = EpocId {
        epoc_id: 0,
        current_block_time: env.block.time,
    };
    save_epoc(&mut deps.storage).save(&first_epoc)?;

    //store total amount zero for the first epoc
    store_total_amount(&mut deps.storage, first_epoc.epoc_id, Uint128::zero())?;

    let all_poc = AllEpoc {
        epoces: vec![first_epoc],
    };
    //store the current epoc on the all epoc storage
    save_all_epoc(&mut deps.storage).save(&all_poc)?;

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
        HandleMsg::ClaimRewards { to } => handle_reward(deps, env, to),
        HandleMsg::Send { recipient, amount } => handle_send(deps, env, recipient, amount),
        HandleMsg::InitBurn { amount } => handle_burn(deps, env, amount),
        HandleMsg::FinishBurn { amount } => handle_finish(deps, env, amount),
        HandleMsg::Register {} => handle_register(deps, env),
        HandleMsg::RegisterValidator { validator } => handle_reg_validator(deps, env, validator),
        HandleMsg::DeRegisterValidator { validator } => {
            handle_dereg_validator(deps, env, validator)
        }
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

    let is_valid = is_valid_validator(&deps.storage, validator.clone())?;
    if !is_valid {
        return Err(StdError::generic_err("Unsupported validator"));
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

    // save the validator storage
    // check whether the validator has previous record on the delegation map
    let mut vld_amount: Uint128 = if read_delegation_map(&deps.storage, validator.clone()).is_err()
    {
        Uint128::zero()
    } else {
        read_delegation_map(&deps.storage, validator.clone())?
    };
    vld_amount += amount;
    store_delegation_map(&mut deps.storage, validator.clone(), vld_amount)?;

    //save the holder map
    store_holder_map(&mut deps.storage, sender.clone(), reward_index)?;

    let res = HandleResponse {
        messages: vec![StakingMsg::Delegate {
            validator,
            amount: payment.clone(),
        }
        .into()],
        log: vec![
            log("action", "mint"),
            log("from", sender),
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
    to: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    let sender: HumanAddr;
    let rcpt_raw: CanonicalAddr;
    if to.is_none() {
        sender = env.message.sender.clone();
        rcpt_raw = deps.api.canonical_address(&sender)?;
    } else {
        sender = to.expect("The receiver address is some");
        rcpt_raw = deps.api.canonical_address(&sender)?;
    }
    let contract_addr = env.contract.address.clone();

    //Get all holders
    let all_holder: HashMap<HumanAddr, Decimal> = read_holders(&deps.storage)?;

    if !all_holder.contains_key(&sender) {
        return Err(StdError::generic_err(
            "The sender has not requested any tokens",
        ));
    }
    let sender_reward_index = all_holder
        .get(&sender)
        .expect("The sender has not requested any tokens");

    let pool = pool_info_read(&deps.storage).load()?;
    let reward_addr = deps.api.human_address(&pool.reward_account)?;
    let user_balance = balances_read(&deps.storage).load(rcpt_raw.as_slice())?;

    //update the holder index
    store_holder_map(&mut deps.storage, sender.clone(), pool.reward_index)?;

    //Retrive all validators
    let validators: Vec<HumanAddr> = read_validators(&deps.storage)?;

    //Withdraw all rewards from all validators.
    let reward;
    if withdraw_all_rewards(validators, pool.clone(), env.block.time, reward_addr) {
        //swap all the balance of the reward contract to uusd.
        send_swap(contract_addr.clone());
        update_index(deps, contract_addr);
        let general_index = pool.reward_index;
        reward = calculate_reward(general_index, sender_reward_index, user_balance).unwrap();
    } else {
        let general_index = pool.reward_index;
        reward = calculate_reward(general_index, sender_reward_index, user_balance).unwrap();
    }

    //send SendReward message to the reward contract
    let msg = SendReward {
        receiver: sender.clone(),
        amount: reward,
    };

    WasmMsg::Execute {
        contract_addr: deps.api.human_address(&pool.reward_account)?,
        msg: to_binary(&msg).unwrap(),
        send: vec![],
    };

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
    let prev_reward_index = pool.reward_index;
    let total_bonded = pool.total_bond_amount;
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
    let sender = env.message.sender.clone();
    //claim the reward.
    let msg = HandleMsg::ClaimRewards {
        to: Some(sender.clone()),
    };
    WasmMsg::Execute {
        contract_addr: env.contract.address.clone(),
        msg: to_binary(&msg)?,
        send: vec![],
    };
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let rcpt_raw = deps.api.canonical_address(&recipient)?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;

    //check the balance of the sender
    let sender_balance = balances_read(&deps.storage).load(sender_raw.as_slice())?;
    if sender_balance < amount {
        return Err(StdError::generic_err(
            "The requested amount is more than the user's balance",
        ));
    }

    let pool_inf = pool_info_read(&deps.storage).load()?;
    let global_index = pool_inf.reward_index;

    //retrieve balances and index of the receiver
    let receive_balance = balances_read(&deps.storage).load(rcpt_raw.as_slice())?;

    let sndr_index = read_holder_map(&deps.storage, sender.clone())?;
    let rcp_prv_index = read_holder_map(&deps.storage, recipient.clone())?;

    let rcp_cur_index = compute_receiver_index(amount, receive_balance, sndr_index, rcp_prv_index);

    //change the balance of sender and receiver.
    let mut accounts = balances(deps.storage.borrow_mut());
    accounts.update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount
    })?;
    accounts.update(rcpt_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    //update the index of receiver and the sender
    store_holder_map(&mut deps.storage, sender.clone(), global_index)?;
    store_holder_map(&mut deps.storage, sender, rcp_cur_index)?;

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

    //check the balance of the user
    let sender_balance = balances_read(&deps.storage).load(sender_raw.as_slice())?;
    if sender_balance < amount {
        return Err(StdError::generic_err(
            "The requested amount is more than the user's balance",
        ));
    }

    let mut epoc = epoc_read(&deps.storage).load()?;
    // get all amount that is gathered in a epoc.
    let mut claimed_so_far = read_total_amount(deps.storage.borrow(), epoc.epoc_id)?;

    //claim the reward.
    let msg = HandleMsg::ClaimRewards {
        to: Some(sender_human.clone()),
    };
    WasmMsg::Execute {
        contract_addr: env.contract.address.clone(),
        msg: to_binary(&msg)?,
        send: vec![],
    };

    // get_all epoces
    let mut all_epoces = read_all_epocs(&deps.storage).load()?;

    // reduce total_supply
    token_info(&mut deps.storage).update(|mut info| {
        info.total_supply = (info.total_supply - amount)?;
        Ok(info)
    })?;

    //update pool info and calculate the new exchange rate.
    let mut exchange_rate = Decimal::zero();
    pool_info(&mut deps.storage).update(|mut pool_inf| {
        pool_inf.total_bond_amount = Uint128(pool_inf.total_bond_amount.0 - amount.0);
        pool_inf.total_issued = (pool_inf.total_issued - amount)?;
        exchange_rate = if pool_inf.total_bond_amount == Uint128::zero()
            || pool_inf.total_bond_amount == Uint128::zero()
        {
            Decimal::one()
        } else {
            pool_inf.update_exchange_rate();
            pool_inf.exchange_rate
        };

        Ok(pool_inf)
    })?;

    balances(&mut deps.storage).update(sender_raw.as_slice(), |balance: Option<Uint128>| {
        balance.unwrap_or_default() - amount * exchange_rate
    })?;

    //compute Epoc time
    let block_time = env.block.time;
    if epoc.is_epoc_passed(block_time) {
        epoc.epoc_id += (block_time - epoc.current_block_time) / EPOC;
        epoc.current_block_time = block_time;

        //store the epoc in valid epoc.
        all_epoces.epoces.push(epoc);
        //store the new amount for the next epoc
        store_total_amount(&mut deps.storage, epoc.epoc_id, amount)?;

        // send undelegate request
        handle_undelegate(deps, env, claimed_so_far, exchange_rate);
        save_epoc(&mut deps.storage).save(&epoc)?;

        //push epoc_id to all_epoc the storage.
        save_all_epoc(&mut deps.storage).update(|mut epocs| {
            epocs.epoces.push(epoc);
            Ok(epocs)
        })?;
        store_undelegated_wait_list(&mut deps.storage, epoc.epoc_id, sender_human, amount)?;
    } else {
        claimed_so_far = claimed_so_far.add(amount);
        //store the human_address under undelegated_wait_list.
        //check whether there is any prev requests form the same user.
        let mut user_amount =
            if read_undelegated_wait_list(&deps.storage, epoc.epoc_id, sender_human.clone())
                .is_err()
            {
                Uint128::zero()
            } else {
                read_undelegated_wait_list(&deps.storage, epoc.epoc_id, sender_human.clone())?
            };
        user_amount += amount;

        store_undelegated_wait_list(&mut deps.storage, epoc.epoc_id, sender_human, user_amount)?;
        //store the claimed_so_far for the current epoc;
        store_total_amount(&mut deps.storage, epoc.epoc_id, claimed_so_far)?;
    }

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
    amount: Uint128,
    exchange_rate: Decimal,
) {
    let token_inf = token_info_read(&deps.storage).load().unwrap();

    //apply exchange_rate
    let amount_with_exchange_rate = amount * exchange_rate;
    // pick a random validator.
    let all_validators = read_validators(&deps.storage).unwrap();
    let validator = pick_validator(deps, all_validators, amount_with_exchange_rate);

    //the validator delegated amount
    let amount = read_delegation_map(&deps.storage, validator.clone()).unwrap();
    let new_amount = amount.0 - amount_with_exchange_rate.0;

    //update the new delegation for the validator
    store_delegation_map(&mut deps.storage, validator.clone(), Uint128(new_amount)).unwrap();

    //send undelegate message
    let msgs: Vec<StakingMsg> = vec![StakingMsg::Undelegate {
        validator,
        amount: coin(amount_with_exchange_rate.u128(), &token_inf.name),
    }];

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
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let sender_human = env.message.sender.clone();
    let contract_address = env.contract.address.clone();

    //check the liquidation period.
    let epoc = epoc_read(&deps.storage).load()?;
    let block_time = env.block.time;

    // get current epoc id.
    let current_epoc_id = compute_epoc(epoc.epoc_id, epoc.current_block_time, block_time);

    let rcpt_raw = deps.api.canonical_address(&env.message.sender)?;

    // Compute all of burn requests with epoc Id corresponding to 21 (can be changed to arbitrary value) days ago
    let epoc_id = get_before_undelegation_epoc(current_epoc_id);
    let all_epocs = read_all_epocs(&deps.storage).load()?;

    for e in all_epocs.epoces {
        if e.epoc_id < epoc_id {
            let list = read_undelegated_wait_list_for_epoc(&deps.storage, e.epoc_id)?;
            for (address, undelegated_amount) in list {
                let raw_address = deps.api.canonical_address(&address)?;
                claim_store(&mut deps.storage)
                    .update(raw_address.as_slice(), |claim: Option<Uint128>| {
                        Ok(claim.unwrap_or_default() + undelegated_amount)
                    })?;
            }
            //remove epoc_id from the storage.
            save_all_epoc(&mut deps.storage).update(|mut epocs| {
                let position = epocs
                    .epoces
                    .iter()
                    .position(|x| x.epoc_id == e.epoc_id)
                    .unwrap();
                epocs.epoces.remove(position);
                Ok(epocs)
            })?;
        }
    }

    if claim_read(&deps.storage).load(rcpt_raw.as_slice()).is_err() {
        Err(StdError::generic_err(
            "The request has been send before undelegation period",
        ))
    } else {
        let claim_balance = claim_read(&deps.storage).load(rcpt_raw.as_slice())?;

        //The user's request might have processed before. Therefore, we need to check its claim balance.
        if amount <= claim_balance {
            return handle_send_undelegation(amount, sender_human, contract_address);
        }
        Err(StdError::generic_err("The amount is not valid"))
    }
}

pub fn get_before_undelegation_epoc(current_epoc: u64) -> u64 {
    if current_epoc < EPOC_PER_UNDELEGATION_PERIOD {
        return 0;
    }
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
        to_address,
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
    if pool.is_reward_exist {
        return Err(StdError::generic_err("The request is not valid"));
    }
    let raw_sender = deps.api.canonical_address(&env.message.sender)?;
    pool.reward_account = raw_sender.clone();
    pool.is_reward_exist = true;
    pool_info(&mut deps.storage).save(&pool)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "register"), log("sub_contract", raw_sender)],
        data: None,
    };
    Ok(res)
}

pub fn handle_reg_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let token = token_info_read(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if token.creator != sender_raw {
        return Err(StdError::generic_err(
            "Only the creator can send this message",
        ));
    }
    store_white_validators(&mut deps.storage, validator.clone())?;
    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "register_validator"),
            log("validator", validator),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_dereg_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let token = token_info_read(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if token.creator != sender_raw {
        return Err(StdError::generic_err(
            "Only the creator can send this message",
        ));
    }
    remove_white_validators(&mut deps.storage, validator.clone())?;
    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "de_register_validator"),
            log("validator", validator),
        ],
        data: None,
    };
    Ok(res)
}

//Pick a random validator
pub fn pick_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    validators: Vec<HumanAddr>,
    claim: Uint128,
) -> HumanAddr {
    let mut rng = rand::thread_rng();
    //FIXME: consider when the validator does not have the amount.
    // we need to split the request to a Vec<validators>.
    loop {
        let random = rng.gen_range(0, validators.len());
        let validator: HumanAddr = HumanAddr::from(validators.get(random).unwrap());
        let val = read_delegation_map(&deps.storage, validator.clone()).unwrap();
        if val > claim {
            return validator;
        }
    }
}

pub fn compute_epoc(mut epoc_id: u64, prev_time: u64, current_time: u64) -> u64 {
    epoc_id += (current_time - prev_time) / EPOC;
    epoc_id
}

pub fn compute_receiver_index(
    burn_amount: Uint128,
    rcp_bal: Uint128,
    rcp_indx: Decimal,
    sndr_indx: Decimal,
) -> Decimal {
    let nom = burn_amount * sndr_indx + rcp_bal * rcp_indx;
    let denom = burn_amount + rcp_bal;
    Decimal::from_ratio(nom, denom)
}

pub fn send_swap(contract_addr: HumanAddr) {
    //send Swap message to the reward contract
    let msg = Swap {};
    WasmMsg::Execute {
        contract_addr,
        msg: to_binary(&msg).unwrap(),
        send: vec![],
    };
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(&deps, address)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(&deps)?),
        QueryMsg::ExchangeRate {} => to_binary(&query_exg_rate(&deps)?),
        QueryMsg::WhiteListedValidators {} => to_binary(&query_white_validators(&deps)?),
        QueryMsg::AccruedRewards { address } => to_binary(&query_accrued_rewards(&deps, address)?),
        QueryMsg::WithdrawableUnbonded { address } => {
            to_binary(&query_withdrawable_unbonded(&deps, address)?)
        }
    }
}

fn query_balance<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {
    let addr_raw = deps.api.canonical_address(&address)?;
    let balance = balances_read(&deps.storage)
        .may_load(addr_raw.as_slice())?
        .unwrap_or_default();
    Ok(balance)
}

fn query_token_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<TokenInfoResponse> {
    let token_info = token_info_read(&deps.storage).load()?;
    Ok(TokenInfoResponse {
        name: token_info.name,
        symbol: token_info.symbol,
        decimals: token_info.decimals,
        total_supply: token_info.total_supply,
    })
}

fn query_exg_rate<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Decimal> {
    let pool = pool_info_read(&deps.storage).load()?;
    Ok(pool.exchange_rate)
}

fn query_white_validators<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Vec<HumanAddr>> {
    let validators = read_valid_validators(&deps.storage)?;
    Ok(validators)
}

fn query_accrued_rewards<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {
    let pool = pool_info_read(&deps.storage).load()?;
    let addr_raw = deps.api.canonical_address(&address)?;
    let global_index = pool.reward_index;
    let all_holder: HashMap<HumanAddr, Decimal> = read_holders(&deps.storage)?;
    let sender_reward_index = all_holder
        .get(&address)
        .expect("The sender has not requested any tokens");
    let user_balance = balances_read(&deps.storage).load(addr_raw.as_slice())?;
    calculate_reward(global_index, sender_reward_index, user_balance)
}

fn query_withdrawable_unbonded<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    address: HumanAddr,
) -> StdResult<Uint128> {
    let addr_raw = deps.api.canonical_address(&address)?;
    let user_claim = claim_read(&deps.storage)
        .may_load(addr_raw.as_slice())?
        .unwrap_or_default();
    Ok(user_claim)
}
