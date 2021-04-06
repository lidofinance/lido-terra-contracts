use crate::state::{read_config, store_config, store_parameters, Parameters};
use cosmwasm_std::{
    log, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier, StakingMsg,
    StdError, StdResult, Storage,
};

/// Update general parameters
/// Only creator/owner is allowed to execute
#[allow(clippy::too_many_arguments)]
pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_period: Option<u64>,
    unbonding_period: Option<u64>,
    peg_recovery_fee: Option<Decimal>,
    er_threshold: Option<Decimal>,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }

    let params: Parameters = store_parameters(&mut deps.storage).load()?;

    let new_params = Parameters {
        epoch_period: epoch_period.unwrap_or(params.epoch_period),
        underlying_coin_denom: params.underlying_coin_denom,
        unbonding_period: unbonding_period.unwrap_or(params.unbonding_period),
        peg_recovery_fee: peg_recovery_fee.unwrap_or(params.peg_recovery_fee),
        er_threshold: er_threshold.unwrap_or(params.er_threshold),
        reward_denom: params.reward_denom,
    };

    store_parameters(&mut deps.storage).save(&new_params)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_params")],
        data: None,
    };
    Ok(res)
}

#[allow(clippy::too_many_arguments)]
/// Update the config. Update the owner, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    reward_contract: Option<HumanAddr>,
    bluna_token_contract: Option<HumanAddr>,
    stluna_token_contract: Option<HumanAddr>,
    airdrop_registry_contract: Option<HumanAddr>,
    validators_registry_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    // only owner must be able to send this message.
    let conf = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.creator = owner_raw;
            Ok(last_config)
        })?;
    }
    if let Some(reward) = reward_contract {
        let reward_raw = deps.api.canonical_address(&reward)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.reward_dispatcher_contract = Some(reward_raw);
            Ok(last_config)
        })?;

        // register the reward contract for automate reward withdrawal.
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: HumanAddr::default(),
            recipient: Some(reward),
        });
        messages.push(msg);
    }

    if let Some(token) = bluna_token_contract {
        let token_raw = deps.api.canonical_address(&token)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.bluna_token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    if let Some(token) = stluna_token_contract {
        let token_raw = deps.api.canonical_address(&token)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.stluna_token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    if let Some(airdrop) = airdrop_registry_contract {
        let airdrop_raw = deps.api.canonical_address(&airdrop)?;
        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.airdrop_registry_contract = Some(airdrop_raw);
            Ok(last_config)
        })?;
    }

    if let Some(validators_registry) = validators_registry_contract {
        let validators_raw = deps.api.canonical_address(&validators_registry)?;
        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.validators_registry_contract = Some(validators_raw);
            Ok(last_config)
        })?;
    }

    let res = HandleResponse {
        messages,
        log: vec![log("action", "update_config")],
        data: None,
    };
    Ok(res)
}
