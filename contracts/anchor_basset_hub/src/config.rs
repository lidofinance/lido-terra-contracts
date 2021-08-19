use crate::msg::QueryMsg::Config;
use crate::state::{Parameters, CONFIG, PARAMETERS};
use cosmwasm_std::{
    attr, Api, CosmosMsg, Decimal, DepsMut, Env, Extern, MessageInfo, Querier, Response,
    StakingMsg, StdError, StdResult, Storage, String,
};

/// Update general parameters
/// Only creator/owner is allowed to execute
#[allow(clippy::too_many_arguments)]
pub fn execute_update_params(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    epoch_period: Option<u64>,
    unbonding_period: Option<u64>,
    peg_recovery_fee: Option<Decimal>,
    er_threshold: Option<Decimal>,
) -> StdResult<Response> {
    // only owner can send this message.
    let config = CONFIG.load(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
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

    let res = Response::new().add_attributes(vec![attr("action", "update_params")]);
    Ok(res)
}

#[allow(clippy::too_many_arguments)]
/// Update the config. Update the owner, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn execute_update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: Option<String>,
    rewards_dispatcher_contract: Option<String>,
    bluna_token_contract: Option<String>,
    stluna_token_contract: Option<String>,
    airdrop_registry_contract: Option<String>,
    validators_registry_contract: Option<String>,
) -> StdResult<Response> {
    // only owner must be able to send this message.
    let conf = CONFIG.load(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(&o)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.creator = owner_raw;
            Ok(last_config)
        })?;
    }
    if let Some(reward) = rewards_dispatcher_contract {
        let reward_raw = deps.api.addr_canonicalize(&reward)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.reward_dispatcher_contract = Some(reward_raw);
            Ok(last_config)
        })?;

        // register the reward contract for automate reward withdrawal.
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: String::default(),
            recipient: Some(reward),
        });
        messages.push(msg);
    }

    if let Some(token) = bluna_token_contract {
        let token_raw = deps.api.addr_canonicalize(&token)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.bluna_token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    if let Some(token) = stluna_token_contract {
        let token_raw = deps.api.addr_canonicalize(&token)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.stluna_token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    if let Some(airdrop) = airdrop_registry_contract {
        let airdrop_raw = deps.api.addr_canonicalize(&airdrop)?;
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.airdrop_registry_contract = Some(airdrop_raw);
            Ok(last_config)
        })?;
    }

    if let Some(validators_registry) = validators_registry_contract {
        let validators_raw = deps.api.addr_canonicalize(&validators_registry)?;
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.validators_registry_contract = Some(validators_raw);
            Ok(last_config)
        })?;
    }

    let res = Response::new().add_attributes(vec![attr("action", "update_config")]);
    Ok(res)
}
