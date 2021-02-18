use crate::state::{
    read_config, read_validators, remove_white_validators, store_config, store_parameters,
    store_white_validators, Parameters,
};
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier,
    StakingMsg, StdError, StdResult, Storage, WasmMsg,
};
use hub_querier::HandleMsg;
use rand::{Rng, SeedableRng, XorShiftRng};

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

/// Update the config. Update the owner, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    reward_contract: Option<HumanAddr>,
    token_contract: Option<HumanAddr>,
    airdrop_registry_contract: Option<HumanAddr>,
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
            last_config.reward_contract = Some(reward_raw);
            Ok(last_config)
        })?;

        // register the reward contract for automate reward withdrawal.
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: HumanAddr::default(),
            recipient: Some(reward),
        });
        messages.push(msg);
    }

    if let Some(token) = token_contract {
        let token_raw = deps.api.canonical_address(&token)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.token_contract = Some(token_raw);
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

    let res = HandleResponse {
        messages,
        log: vec![log("action", "update_config")],
        data: None,
    };
    Ok(res)
}

/// Register a white listed validator.
/// Only creator/owner is allowed to execute
pub fn handle_register_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let hub_conf = read_config(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    let contract_raw = deps.api.canonical_address(&env.contract.address)?;
    if hub_conf.creator != sender_raw && contract_raw != sender_raw {
        return Err(StdError::unauthorized());
    }

    // given validator must be first a validator in the system.
    let exists = deps
        .querier
        .query_validators()?
        .iter()
        .any(|val| val.address == validator);
    if !exists {
        return Err(StdError::generic_err(
            "The specified address is not a validator",
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

/// Deregister a previously-whitelisted validator.
/// Only creator/owner is allowed to execute
pub fn handle_deregister_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    let token = read_config(&deps.storage).load()?;

    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if token.creator != sender_raw {
        return Err(StdError::unauthorized());
    }
    let validators_before_remove = read_validators(&deps.storage)?;

    if validators_before_remove.len() == 1 {
        return Err(StdError::generic_err(
            "Cannot remove the last whitelisted validator",
        ));
    }

    remove_white_validators(&mut deps.storage, validator.clone())?;

    let query = deps
        .querier
        .query_delegation(env.contract.address.clone(), validator.clone());

    let mut replaced_val = HumanAddr::default();
    let mut messages: Vec<CosmosMsg> = vec![];

    if let Ok(q) = query {
        let delegated_amount = q;
        let validators = read_validators(&deps.storage)?;

        // redelegate the amount to a random validator.
        let block_height = env.block.height;
        let mut rng = XorShiftRng::seed_from_u64(block_height);
        let random_index = rng.gen_range(0, validators.len());
        replaced_val = HumanAddr::from(validators.get(random_index).unwrap());

        if let Some(delegation) = delegated_amount {
            messages.push(CosmosMsg::Staking(StakingMsg::Redelegate {
                src_validator: validator.clone(),
                dst_validator: replaced_val.clone(),
                amount: delegation.amount,
            }));

            let msg = HandleMsg::UpdateGlobalIndex {
                airdrop_hooks: None,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address,
                msg: to_binary(&msg)?,
                send: vec![],
            }));
        }
    }

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "de_register_validator"),
            log("validator", validator),
            log("new-validator", replaced_val),
        ],
        data: None,
    };
    Ok(res)
}
