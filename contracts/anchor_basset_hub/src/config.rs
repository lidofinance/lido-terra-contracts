use crate::state::{
    config, config_read, msg_status, parameters, read_validators, remove_white_validators,
    store_white_validators, Parameters,
};
use anchor_basset_reward::msg::HandleMsg::UpdateParams;
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier,
    StakingMsg, StdError, StdResult, Storage, WasmMsg,
};
use hub_querier::{Deactivated, HandleMsg, Registration};
use rand::{Rng, SeedableRng, XorShiftRng};

#[allow(clippy::too_many_arguments)]
pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_period: Option<u64>,
    underlying_coin_denom: Option<String>,
    unbonding_period: Option<u64>,
    peg_recovery_fee: Option<Decimal>,
    er_threshold: Option<Decimal>,
    reward_denom: Option<String>,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }

    let params: Parameters = parameters(&mut deps.storage).load()?;
    let new_params = Parameters {
        epoch_period: epoch_period.unwrap_or(params.epoch_period),
        underlying_coin_denom: underlying_coin_denom.unwrap_or(params.underlying_coin_denom),
        unbonding_period: unbonding_period.unwrap_or(params.unbonding_period),
        peg_recovery_fee: peg_recovery_fee.unwrap_or(params.peg_recovery_fee),
        er_threshold: er_threshold.unwrap_or(params.er_threshold),
        reward_denom: reward_denom.clone().unwrap_or(params.reward_denom),
    };

    let mut msgs: Vec<CosmosMsg> = vec![];
    if let Some(denom) = reward_denom {
        let reward_addr = deps.api.human_address(
            &config
                .reward_contract
                .expect("the reward contract must have been registered"),
        )?;

        //send update params to the reward contract
        let set_swap = UpdateParams {
            reward_denom: Some(denom),
        };

        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_addr,
            msg: to_binary(&set_swap)?,
            send: vec![],
        }));
    }

    parameters(&mut deps.storage).save(&new_params)?;
    let res = HandleResponse {
        messages: msgs,
        log: vec![log("action", "update_params")],
        data: None,
    };
    Ok(res)
}

pub fn handle_deactivate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: Deactivated,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }

    match msg {
        Deactivated::Slashing => {
            msg_status(&mut deps.storage).update(|mut msg_status| {
                msg_status.slashing = Some(msg);
                Ok(msg_status)
            })?;
        }
        Deactivated::Unbond => {
            msg_status(&mut deps.storage).update(|mut msg_status| {
                msg_status.unbond = Some(msg);
                Ok(msg_status)
            })?;
        }
    }

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "deactivate_msg")],
        data: None,
    };
    Ok(res)
}

pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    reward_contract: Option<HumanAddr>,
    token_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    let conf = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;

        config(&mut deps.storage).update(|mut last_config| {
            last_config.creator = owner_raw;
            Ok(last_config)
        })?;
    }
    if let Some(reward) = reward_contract {
        let reward_raw = deps.api.canonical_address(&reward)?;

        config(&mut deps.storage).update(|mut last_config| {
            last_config.reward_contract = Some(reward_raw);
            Ok(last_config)
        })?;
    }

    if let Some(token) = token_contract {
        let token_raw = deps.api.canonical_address(&token)?;

        config(&mut deps.storage).update(|mut last_config| {
            last_config.token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "change_the_owner")],
        data: None,
    };
    Ok(res)
}

pub fn handle_register_contracts<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: Registration,
    contract_address: HumanAddr,
) -> StdResult<HandleResponse> {
    let conf = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let raw_contract_addr = deps.api.canonical_address(&contract_address)?;
    let mut messages: Vec<CosmosMsg> = vec![];

    match contract {
        Registration::Reward => {
            if conf.reward_contract.is_some() {
                return Err(StdError::generic_err(
                    "The reward contract is already registered",
                ));
            }
            config(&mut deps.storage).update(|mut last_config| {
                last_config.reward_contract = Some(raw_contract_addr.clone());
                Ok(last_config)
            })?;
            let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
                validator: HumanAddr::default(),
                recipient: Some(deps.api.human_address(&raw_contract_addr)?),
            });
            messages.push(msg);
        }
        Registration::Token => {
            config(&mut deps.storage).update(|mut last_config| {
                if last_config.token_contract.is_some() {
                    return Err(StdError::generic_err(
                        "The token contract is already registered",
                    ));
                }
                last_config.token_contract = Some(raw_contract_addr.clone());
                Ok(last_config)
            })?;
        }
    }
    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "register"),
            log("sub_contract", contract_address),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_reg_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let hub_conf = config_read(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if hub_conf.creator != sender_raw {
        return Err(StdError::generic_err(
            "Only the creator can send this message",
        ));
    }

    let exists = deps
        .querier
        .query_validators()?
        .iter()
        .any(|val| val.address == validator);
    if !exists {
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
