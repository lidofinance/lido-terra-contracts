use cosmwasm_std::{
    coin, coins, from_binary, log, to_binary, Api, BankMsg, Binary, CosmosMsg, Decimal, Env,
    Extern, HandleResponse, HumanAddr, InitResponse, Querier, StakingMsg, StdError, StdResult,
    Storage, Uint128, WasmMsg,
};

use crate::msg::{InitMsg, QueryMsg};
use crate::state::{
    config, config_read, epoc_read, get_all_delegations, get_finished_amount, get_minted,
    is_valid_validator, pool_info, pool_info_read, read_total_amount, read_undelegated_wait_list,
    read_valid_validators, read_validators, remove_white_validators, save_epoc,
    set_all_delegations, set_minted, store_total_amount, store_undelegated_wait_list,
    store_white_validators, EpocId, GovConfig, EPOC,
};
use anchor_basset_reward::hook::InitHook;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::msg::HandleMsg::{Swap, UpdateGlobalIndex};
use anchor_basset_token::msg::HandleMsg::{Burn, Mint};
use anchor_basset_token::msg::{TokenInitHook, TokenInitMsg};
use cw20::{Cw20ReceiveMsg, MinterResponse};
use gov_courier::PoolInfo;
use gov_courier::Registration;
use gov_courier::{Cw20HookMsg, HandleMsg};
use rand::Rng;
use std::ops::Add;

const LUNA: &str = "uluna";
const EPOC_PER_UNDELEGATION_PERIOD: u64 = 83;
const DECIMALS: u8 = 6;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    // validate token info
    msg.validate()?;

    // store token info
    let sender = env.message.sender;
    let sndr_raw = deps.api.canonical_address(&sender)?;
    let data = GovConfig { creator: sndr_raw };
    config(&mut deps.storage).save(&data)?;

    let pool = PoolInfo {
        exchange_rate: Decimal::one(),
        last_index_modification: env.block.time,
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

    let mut messages: Vec<CosmosMsg> = vec![];

    let gov_address = env.contract.address;
    let token_message = to_binary(&HandleMsg::RegisterSubContracts {
        contract: Registration::Token,
    })?;

    //set minted and all_delegations to keep the record of slashing.
    set_minted(&mut deps.storage).save(&Uint128::zero())?;
    set_all_delegations(&mut deps.storage).save(&Uint128::zero())?;

    //instantiate token contract
    messages.push(CosmosMsg::Wasm(WasmMsg::Instantiate {
        code_id: msg.token_code_id,
        msg: to_binary(&TokenInitMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: DECIMALS,
            initial_balances: vec![],
            owner: deps.api.canonical_address(&gov_address)?,
            init_hook: Some(TokenInitHook {
                msg: token_message,
                contract_addr: gov_address.clone(),
            }),
            mint: Some(MinterResponse {
                minter: gov_address.clone(),
                cap: None,
            }),
        })?,
        send: vec![],
        label: None,
    }));

    //instantiate reward contract
    let reward_message = to_binary(&HandleMsg::RegisterSubContracts {
        contract: Registration::Reward,
    })?;
    messages.push(CosmosMsg::Wasm(WasmMsg::Instantiate {
        code_id: msg.reward_code_id,
        msg: to_binary(&RewardInitMsg {
            owner: deps.api.canonical_address(&gov_address)?,
            init_hook: Some(InitHook {
                msg: reward_message,
                contract_addr: gov_address,
            }),
        })?,
        send: vec![],
        label: None,
    }));

    let res = InitResponse {
        messages,
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
        HandleMsg::Receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::Mint { validator } => handle_mint(deps, env, validator),
        HandleMsg::UpdateGlobalIndex {} => handle_update_global(deps, env),
        HandleMsg::FinishBurn { amount } => handle_finish(deps, env, amount),
        HandleMsg::RegisterSubContracts { contract } => {
            handle_register_contracts(deps, env, contract)
        }
        HandleMsg::RegisterValidator { validator } => handle_reg_validator(deps, env, validator),
        HandleMsg::DeRegisterValidator { validator } => {
            handle_dereg_validator(deps, env, validator)
        }
        HandleMsg::ReportSlashing {} => handle_slashing(deps, env),
    }
}

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> StdResult<HandleResponse> {
    let contract_addr = env.message.sender.clone();

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::InitBurn {} => {
                // only asset contract can execute this message
                let pool = pool_info_read(&deps.storage).load()?;
                if deps.api.canonical_address(&contract_addr)? != pool.token_account {
                    return Err(StdError::unauthorized());
                }
                handle_burn(deps, env, cw20_msg.amount, cw20_msg.sender)
            }
        }
    } else {
        Err(StdError::generic_err("Invalid request"))
    }
}

pub fn handle_mint<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let is_valid = is_valid_validator(&deps.storage, validator.clone())?;
    if !is_valid {
        return Err(StdError::generic_err("Unsupported validator"));
    }

    //Check whether the account has sent the native coin in advance.
    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == LUNA && x.amount > Uint128::zero())
        .ok_or_else(|| StdError::generic_err(format!("No {} tokens sent", LUNA)))?;

    let mut pool = pool_info_read(&deps.storage).load()?;
    let sender = env.message.sender;

    let amount_with_exchange_rate =
        if pool.total_bond_amount.is_zero() || pool.total_issued.is_zero() {
            payment.amount
        } else {
            pool.update_exchange_rate();
            let exchange_rate = pool.exchange_rate;
            exchange_rate * payment.amount
        };

    //update pool_info
    pool.total_bond_amount += amount_with_exchange_rate;
    pool.total_issued += amount_with_exchange_rate;

    pool_info(&mut deps.storage).save(&pool)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // Issue the bluna token for sender
    let mint_msg = Mint {
        recipient: sender.clone(),
        amount: amount_with_exchange_rate,
    };
    let token_address = deps.api.human_address(&pool.token_account)?;
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&mint_msg)?,
        send: vec![],
    }));

    //delegate the amount
    messages.push(CosmosMsg::Staking(StakingMsg::Delegate {
        validator,
        amount: payment.clone(),
    }));

    //add minted for slashing
    set_minted(&mut deps.storage).update(|mut mint| {
        mint += amount_with_exchange_rate;
        Ok(mint)
    })?;

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "mint"),
            log("from", sender),
            log("bonded", payment.amount),
            log("minted", amount_with_exchange_rate),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_update_global<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let mut messages: Vec<CosmosMsg> = vec![];

    let pool = pool_info_read(&deps.storage).load()?;
    let reward_addr = deps.api.human_address(&pool.reward_account)?;

    //retrieve all validators
    let validators: Vec<HumanAddr> = read_validators(&deps.storage)?;

    //send withdraw message
    let mut withdraw_msgs = withdraw_all_rewards(validators);
    messages.append(&mut withdraw_msgs);

    //send Swap message to reward contract
    let swap_msg = Swap {};
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr.clone(),
        msg: to_binary(&swap_msg).unwrap(),
        send: vec![],
    }));

    //send update GlobalIndex message to reward contract
    let global_msg = UpdateGlobalIndex {};
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: reward_addr,
        msg: to_binary(&global_msg).unwrap(),
        send: vec![],
    }));

    //update pool_info last modified
    pool_info(&mut deps.storage).update(|mut pool| {
        pool.last_index_modification = env.block.time;
        Ok(pool)
    })?;

    let res = HandleResponse {
        messages,
        log: vec![log("action", "claim_reward")],
        data: None,
    };
    Ok(res)
}

//create withdraw requests for all validators
pub fn withdraw_all_rewards(validators: Vec<HumanAddr>) -> Vec<CosmosMsg> {
    let mut messages: Vec<CosmosMsg> = vec![];
    for val in validators {
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: val,
            recipient: None,
        });
        messages.push(msg)
    }
    messages
}

// calculate the reward based on the sender's index and the global index.
pub fn calculate_reward(
    general_index: Decimal,
    user_index: &Decimal,
    user_balance: Uint128,
) -> StdResult<Uint128> {
    general_index * user_balance - *user_index * user_balance
}

pub fn handle_burn<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
) -> StdResult<HandleResponse> {
    if amount == Uint128::zero() {
        return Err(StdError::generic_err("Invalid zero amount"));
    }

    let mut epoc = epoc_read(&deps.storage).load()?;
    // get all amount that is gathered in a epoc.
    let mut undelegated_so_far = read_total_amount(&deps.storage, epoc.epoc_id)?;

    let mut messages: Vec<CosmosMsg> = vec![];

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

    let pool = pool_info_read(&deps.storage).load()?;

    //send Burn message to token contract
    let token_address = deps.api.human_address(&pool.token_account)?;
    let burn_msg = Burn { amount };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&burn_msg)?,
        send: vec![],
    }));

    //compute Epoc time
    let block_time = env.block.time;
    if epoc.is_epoc_passed(block_time) {
        epoc.epoc_id += (block_time - epoc.current_block_time) / EPOC;
        epoc.current_block_time = block_time;

        //store the new amount for the next epoc
        store_total_amount(&mut deps.storage, epoc.epoc_id, amount)?;

        let delegator = env.contract.address;

        // send undelegated requests
        let mut undelegated_msgs =
            handle_undelegate(deps, undelegated_so_far, exchange_rate, delegator);
        messages.append(&mut undelegated_msgs);
        save_epoc(&mut deps.storage).save(&epoc)?;

        store_undelegated_wait_list(&mut deps.storage, epoc.epoc_id, sender.clone(), amount)?;
    } else {
        undelegated_so_far = undelegated_so_far.add(amount);
        //store the human_address under undelegated_wait_list.
        //check whether there is any prev requests form the same user.
        let mut user_amount =
            if read_undelegated_wait_list(&deps.storage, epoc.epoc_id, sender.clone()).is_err() {
                Uint128::zero()
            } else {
                read_undelegated_wait_list(&deps.storage, epoc.epoc_id, sender.clone())?
            };
        user_amount += amount;

        store_undelegated_wait_list(&mut deps.storage, epoc.epoc_id, sender.clone(), user_amount)?;
        //store the claimed_so_far for the current epoc;
        store_total_amount(&mut deps.storage, epoc.epoc_id, undelegated_so_far)?;
    }

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "burn"),
            log("from", sender),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_undelegate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    amount: Uint128,
    exchange_rate: Decimal,
    delegator: HumanAddr,
) -> Vec<CosmosMsg> {
    //apply exchange_rate
    let amount_with_exchange_rate = amount * exchange_rate;
    // pick a random validator.
    let all_validators = read_validators(&deps.storage).unwrap();
    pick_validator(deps, all_validators, delegator, amount_with_exchange_rate)
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

    // Compute all of burn requests with epoc Id corresponding to 21 (can be changed to arbitrary value) days ago
    let epoc_id = get_before_undelegation_epoc(current_epoc_id);

    let amount = get_finished_amount(&deps.storage, epoc_id, sender_human.clone())?;
    handle_slashing(deps, env)?;
    let exchange_rate = pool_info_read(&deps.storage).load()?.exchange_rate;
    let final_amount = amount * exchange_rate;
    handle_send_undelegation(final_amount, sender_human, contract_address)
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

pub fn handle_register_contracts<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: Registration,
) -> StdResult<HandleResponse> {
    let raw_sender = deps.api.canonical_address(&env.message.sender)?;
    let mut messages: Vec<CosmosMsg> = vec![];
    match contract {
        Registration::Reward => {
            let mut pool = pool_info_read(&deps.storage).load()?;
            if pool.is_reward_exist {
                return Err(StdError::generic_err("The request is not valid"));
            }
            pool.reward_account = raw_sender.clone();
            pool.is_reward_exist = true;
            pool_info(&mut deps.storage).save(&pool)?;

            let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
                validator: HumanAddr::default(),
                recipient: Some(deps.api.human_address(&raw_sender)?),
            });
            messages.push(msg);
        }
        Registration::Token => {
            pool_info(&mut deps.storage).update(|mut pool| {
                if pool.is_token_exist {
                    return Err(StdError::generic_err("The request is not valid"));
                }
                pool.token_account = raw_sender.clone();
                pool.is_token_exist = true;
                Ok(pool)
            })?;
        }
    }
    let res = HandleResponse {
        messages,
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
    let gov_conf = config_read(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if gov_conf.creator != sender_raw {
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
    let token = config_read(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if token.creator != sender_raw {
        return Err(StdError::generic_err(
            "Only the creator can send this message",
        ));
    }
    remove_white_validators(&mut deps.storage, validator.clone())?;

    let query = deps
        .querier
        .query_delegation(env.contract.address.clone(), validator.clone())?
        .unwrap();
    let delegated_amount = query.amount;

    let mut messages: Vec<CosmosMsg> = vec![];
    let validators = read_validators(&deps.storage)?;

    //redelegate the amount to a random validator.
    let mut rng = rand::thread_rng();
    let random = rng.gen_range(0, validators.len());
    let replaced_val = HumanAddr::from(validators.get(random).unwrap());
    messages.push(CosmosMsg::Staking(StakingMsg::Redelegate {
        src_validator: validator.clone(),
        dst_validator: replaced_val,
        amount: delegated_amount,
    }));

    let msg = HandleMsg::UpdateGlobalIndex {};
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address,
        msg: to_binary(&msg)?,
        send: vec![],
    }));

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "de_register_validator"),
            log("validator", validator),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let validators = read_validators(&deps.storage)?;
    let mut amount = Uint128::zero();
    let all_delegations = get_all_delegations(&deps.storage).load()?;
    let minted = get_minted(&deps.storage).load()?;
    for validator in validators {
        let slashing = deps
            .querier
            .query_delegation(env.contract.address.clone(), validator)?
            .unwrap();
        if slashing.amount.denom == "uluna" {
            amount += slashing.amount.amount;
        }
    }
    let all_changes = amount.0 - all_delegations.0;
    if minted.0 > all_changes {
        pool_info(&mut deps.storage).update(|mut pool| {
            pool.total_bond_amount = amount;
            pool.update_exchange_rate();
            Ok(pool)
        })?;
    }
    set_all_delegations(&mut deps.storage).save(&amount)?;
    set_minted(&mut deps.storage).save(&Uint128::zero())?;
    Ok(HandleResponse::default())
}

//Pick a random validator
pub fn pick_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    validators: Vec<HumanAddr>,
    delegator: HumanAddr,
    claim: Uint128,
) -> Vec<CosmosMsg> {
    let mut claimed = claim;
    let mut rng = rand::thread_rng();
    let mut messages: Vec<CosmosMsg> = vec![];
    let mut situation: Option<bool> = None;
    for v in validators.clone() {
        let val = deps
            .querier
            .query_delegation(delegator.clone(), v.clone())
            .unwrap()
            .unwrap()
            .amount
            .amount;
        if val < claim {
            situation = None;
        } else {
            situation = Some(true);
            let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: v,
                amount: coin(claim.u128(), LUNA),
            });
            messages.push(msgs);
        }
    }
    if situation.is_none() {
        while claimed.0 > 0 {
            let random = rng.gen_range(0, validators.len());
            let validator: HumanAddr = HumanAddr::from(validators.get(random).unwrap());
            let val = deps
                .querier
                .query_delegation(delegator.clone(), validator.clone())
                .unwrap()
                .unwrap()
                .amount
                .amount;
            claimed = Uint128(claim.0 - val.0);
            let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
                validator,
                amount: coin(val.u128(), LUNA),
            });
            messages.push(msgs);
        }
    }
    messages
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
        QueryMsg::ExchangeRate {} => to_binary(&query_exg_rate(&deps)?),
        QueryMsg::WhiteListedValidators {} => to_binary(&query_white_validators(&deps)?),
        QueryMsg::WithdrawableUnbonded { address } => {
            to_binary(&query_withdrawable_unbonded(&deps, address)?)
        }
        QueryMsg::GetToken {} => to_binary(&query_token(&deps)?),
        QueryMsg::GetReward {} => to_binary(&query_reward(&deps)?),
    }
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

fn query_withdrawable_unbonded<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>,
    _address: HumanAddr,
) -> StdResult<Uint128> {
    unimplemented!()
}

fn query_token<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<HumanAddr> {
    let pool = pool_info_read(&deps.storage).load()?;
    deps.api.human_address(&pool.token_account)
}

fn query_reward<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<HumanAddr> {
    let pool = pool_info_read(&deps.storage).load()?;
    deps.api.human_address(&pool.reward_account)
}
