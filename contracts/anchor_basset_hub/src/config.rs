use crate::state::{
    read_validators, remove_white_validators, store_white_validators, Parameters, CONFIG,
    PARAMETERS,
};
use basset::hub::{Config, ExecuteMsg};
use cosmwasm_std::{
    attr, to_binary, Addr, CosmosMsg, Decimal, DepsMut, DistributionMsg, Env, MessageInfo,
    Response, StakingMsg, StdError, StdResult, WasmMsg,
};

use rand::{Rng, SeedableRng, XorShiftRng};

/// Update general parameters
/// Only creator/owner is allowed to execute
#[allow(clippy::too_many_arguments)]
pub fn execute_update_params(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    epoch_period: Option<u64>,
    unbonding_period: Option<u64>,
    peg_recovery_fee: Option<Decimal>,
    er_threshold: Option<Decimal>,
) -> StdResult<Response> {
    // only owner can send this message.
    let config = CONFIG.load(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    if sender_raw != config.creator {
        return Err(StdError::generic_err("unauthorized"));
    }

    let params: Parameters = PARAMETERS.load(deps.storage)?;

    let new_params = Parameters {
        epoch_period: epoch_period.unwrap_or(params.epoch_period),
        underlying_coin_denom: params.underlying_coin_denom,
        unbonding_period: unbonding_period.unwrap_or(params.unbonding_period),
        peg_recovery_fee: peg_recovery_fee.unwrap_or(params.peg_recovery_fee),
        er_threshold: er_threshold.unwrap_or(params.er_threshold),
        reward_denom: params.reward_denom,
    };

    PARAMETERS.save(deps.storage, &new_params)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_params")]))
}

/// Update the config. Update the owner, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    reward_contract: Option<String>,
    token_contract: Option<String>,
    airdrop_registry_contract: Option<String>,
) -> StdResult<Response> {
    // only owner must be able to send this message.
    let conf = CONFIG.load(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    if sender_raw != conf.creator {
        return Err(StdError::generic_err("unauthorized"));
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(o.as_str())?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<Config> {
            last_config.creator = owner_raw;
            Ok(last_config)
        })?;
    }
    if let Some(reward) = reward_contract {
        let reward_raw = deps.api.addr_canonicalize(reward.as_str())?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<Config> {
            last_config.reward_contract = Some(reward_raw);
            Ok(last_config)
        })?;

        // register the reward contract for automate reward withdrawal.
        messages.push(CosmosMsg::Distribution(
            DistributionMsg::SetWithdrawAddress { address: reward },
        ));
    }

    if let Some(token) = token_contract {
        let token_raw = deps.api.addr_canonicalize(token.as_str())?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<Config> {
            last_config.token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    if let Some(airdrop) = airdrop_registry_contract {
        let airdrop_raw = deps.api.addr_canonicalize(airdrop.as_str())?;
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<Config> {
            last_config.airdrop_registry_contract = Some(airdrop_raw);
            Ok(last_config)
        })?;
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "update_config")]))
}

/// Register a white listed validator.
/// Only creator/owner is allowed to execute
pub fn execute_register_validator(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    validator: String,
) -> StdResult<Response> {
    let hub_conf = CONFIG.load(deps.storage)?;

    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    let contract_raw = deps.api.addr_canonicalize(env.contract.address.as_str())?;
    if hub_conf.creator != sender_raw && contract_raw != sender_raw {
        return Err(StdError::generic_err("unauthorized"));
    }

    // given validator must be first a validator in the system.
    let exists = deps
        .querier
        .query_all_validators()?
        .iter()
        .any(|val| val.address == validator);
    if !exists {
        return Err(StdError::generic_err(
            "The specified address is not a validator",
        ));
    }

    store_white_validators(deps.storage, validator.clone())?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "register_validator"),
        attr("validator", validator),
    ]))
}

/// Deregister a previously-whitelisted validator.
/// Only creator/owner is allowed to execute
pub fn execute_deregister_validator(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    validator: String,
) -> StdResult<Response> {
    let token = CONFIG.load(deps.storage)?;

    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    if token.creator != sender_raw {
        return Err(StdError::generic_err("unauthorized"));
    }
    let validators_before_remove = read_validators(deps.storage)?;

    if validators_before_remove.len() == 1 {
        return Err(StdError::generic_err(
            "Cannot remove the last whitelisted validator",
        ));
    }

    remove_white_validators(deps.storage, validator.to_string())?;

    let query = deps
        .querier
        .query_delegation(env.contract.address.clone(), validator.clone());

    let mut replaced_val = Addr::unchecked("");
    let mut messages: Vec<CosmosMsg> = vec![];

    if let Ok(q) = query {
        let delegated_amount = q;
        let validators = read_validators(deps.storage)?;

        // redelegate the amount to a random validator.
        let block_height = env.block.height;
        let mut rng = XorShiftRng::seed_from_u64(block_height);
        let random_index = rng.gen_range(0, validators.len());
        replaced_val = Addr::unchecked(validators.get(random_index).unwrap().as_str());

        if let Some(delegation) = delegated_amount {
            messages.push(CosmosMsg::Staking(StakingMsg::Redelegate {
                src_validator: validator.to_string(),
                dst_validator: replaced_val.to_string(),
                amount: delegation.amount,
            }));

            let msg = ExecuteMsg::UpdateGlobalIndex {
                airdrop_hooks: None,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            }));
        }
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "de_register_validator"),
        attr("validator", validator),
        attr("new-validator", replaced_val),
    ]))
}
