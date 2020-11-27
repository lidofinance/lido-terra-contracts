use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, InitResponse, Querier, QueryRequest, StakingMsg, StdError, StdResult, Storage,
    Uint128, WasmMsg, WasmQuery,
};

use crate::burn::{handle_burn, handle_finish};
use crate::config::{handle_deactivate, handle_update_params};
use crate::math::{decimal_division, decimal_subtraction};
use crate::msg::{InitMsg, QueryMsg};
use crate::state::{
    config, config_read, get_all_delegations, get_bonded, is_valid_validator, msg_status,
    msg_status_read, parameters_read, pool_info, pool_info_read, read_valid_validators,
    read_validators, remove_white_validators, save_epoch, set_all_delegations, set_bonded,
    store_total_amount, store_white_validators, EpochId, GovConfig, MsgStatus, Parameters,
};
use anchor_basset_reward::hook::InitHook;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::msg::HandleMsg::{Swap, UpdateGlobalIndex};
use anchor_basset_token::msg::{TokenInitHook, TokenInitMsg};
use anchor_basset_token::state::TokenInfo;
use cosmwasm_storage::to_length_prefixed;
use cw20::{Cw20CoinHuman, Cw20HandleMsg, Cw20ReceiveMsg, MinterResponse};
use gov_courier::PoolInfo;
use gov_courier::Registration;
use gov_courier::{Cw20HookMsg, HandleMsg};
use rand::{Rng, SeedableRng, XorShiftRng};

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

    //store the first epoch.
    let first_epoch = EpochId {
        epoch_id: 0,
        current_block_time: env.block.time,
    };
    save_epoch(&mut deps.storage).save(&first_epoch)?;

    //store total amount zero for the first epoc
    store_total_amount(&mut deps.storage, first_epoch.epoch_id, Uint128::zero())?;

    //store none for burn and finish deactivate status
    let msg_state = MsgStatus {
        slashing: None,
        burn: None,
    };
    msg_status(&mut deps.storage).save(&msg_state)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    let gov_address = env.contract.address;
    let token_message = to_binary(&HandleMsg::RegisterSubContracts {
        contract: Registration::Token,
    })?;

    //set minted and all_delegations to keep the record of slashing.
    set_bonded(&mut deps.storage).save(&Uint128::zero())?;
    set_all_delegations(&mut deps.storage).save(&Uint128::zero())?;

    //instantiate token contract
    messages.push(CosmosMsg::Wasm(WasmMsg::Instantiate {
        code_id: msg.token_code_id,
        msg: to_binary(&TokenInitMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: DECIMALS,
            initial_balances: vec![Cw20CoinHuman {
                address: gov_address.clone(),
                amount: Uint128(0),
            }],
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
        HandleMsg::FinishBurn {} => handle_finish(deps, env),
        HandleMsg::RegisterSubContracts { contract } => {
            handle_register_contracts(deps, env, contract)
        }
        HandleMsg::RegisterValidator { validator } => handle_reg_validator(deps, env, validator),
        HandleMsg::DeRegisterValidator { validator } => {
            handle_dereg_validator(deps, env, validator)
        }
        HandleMsg::ReportSlashing {} => handle_slashing(deps, env),
        HandleMsg::UpdateParams {
            epoch_time,
            coin_denom,
            undelegated_epoch,
            peg_recovery_fee,
            er_threshold,
        } => handle_update_params(
            deps,
            env,
            epoch_time,
            coin_denom,
            undelegated_epoch,
            peg_recovery_fee,
            er_threshold,
        ),
        HandleMsg::DeactivateMsg { msg } => handle_deactivate(deps, env, msg),
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
                // only token contract can execute this message
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

    //read params
    let params = parameters_read(&deps.storage).load()?;
    let coin_denom = params.supported_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    //read msg_status
    let msg_status = msg_status_read(&deps.storage).load()?;

    //Check whether the account has sent the native coin in advance.
    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| StdError::generic_err(format!("No {} tokens sent", coin_denom)))?;

    //update the exchange rate
    if msg_status.slashing.is_none() && slashing(deps, env.clone()).is_ok() {
        slashing(deps, env.clone())?;
    }

    let mut pool = pool_info_read(&deps.storage).load()?;
    let sender = env.message.sender.clone();

    //apply recovery fee if it is necessary
    let mut amount_with_exchange_rate = decimal_division(payment.amount, pool.exchange_rate);
    if pool.exchange_rate < threshold {
        let peg_fee = decimal_subtraction(Decimal::one(), recovery_fee);
        amount_with_exchange_rate = amount_with_exchange_rate * peg_fee;
    }

    //update pool_info
    pool.total_bond_amount += payment.amount;

    pool_info(&mut deps.storage).save(&pool)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // Issue the bluna token for sender
    let mint_msg = Cw20HandleMsg::Mint {
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
    set_bonded(&mut deps.storage).update(|mut bonded| {
        bonded += payment.amount;
        Ok(bonded)
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
        log: vec![log("action", "update_global_index")],
        data: None,
    };
    Ok(res)
}

//create withdraw requests for all validators
fn withdraw_all_rewards(validators: Vec<HumanAddr>) -> Vec<CosmosMsg> {
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

    let is_exist = deps
        .querier
        .query_validators()?
        .iter()
        .any(|val| val.address == validator);
    if !is_exist {
        return Err(StdError::generic_err("Invalid validator"));
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
    let block_height = env.block.height;
    let mut rng = XorShiftRng::seed_from_u64(block_height);
    let random_index = rng.gen_range(0, validators.len());
    let replaced_val = HumanAddr::from(validators.get(random_index).unwrap());
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

pub fn slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<()> {
    //read params
    let params = parameters_read(&deps.storage).load()?;
    let coin_denom = params.supported_coin_denom;

    let mut amount = Uint128::zero();
    let all_delegations = get_all_delegations(&deps.storage).load()?;
    let bonded = get_bonded(&deps.storage).load()?;
    let all_delegated_amount = deps.querier.query_all_delegations(env.contract.address)?;
    for delegate in all_delegated_amount {
        if delegate.amount.denom == coin_denom {
            amount += delegate.amount.amount
        }
    }
    let all_changes = (amount - all_delegations)?;
    let total_issued = query_total_issued(&deps)?;
    if bonded.0 > all_changes.0 {
        pool_info(&mut deps.storage).update(|mut pool| {
            pool.total_bond_amount = amount;
            pool.update_exchange_rate(total_issued);
            Ok(pool)
        })?;
    }
    set_all_delegations(&mut deps.storage).save(&amount)?;
    set_bonded(&mut deps.storage).save(&Uint128::zero())?;
    Ok(())
}

pub fn handle_slashing<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    //read msg_status
    let msg_status = msg_status_read(&deps.storage).load()?;
    if msg_status.slashing.is_some() {
        return Err(StdError::generic_err(
            "this message is temporarily deactivated",
        ));
    }
    slashing(deps, env)?;
    Ok(HandleResponse::default())
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
        QueryMsg::GetParams {} => to_binary(&query_params(&deps)?),
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

fn query_params<S: Storage, A: Api, Q: Querier>(deps: &Extern<S, A, Q>) -> StdResult<Parameters> {
    parameters_read(&deps.storage).load()
}

fn query_total_issued<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Uint128> {
    let token_address = deps
        .api
        .human_address(&pool_info_read(&deps.storage).load()?.token_account)?;
    let res = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: token_address,
        key: Binary::from(to_length_prefixed(b"token_info")),
    }))?;
    let token_info: TokenInfo = from_binary(&res)?;
    Ok(token_info.total_supply)
}
