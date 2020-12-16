use crate::state::{
    config, config_read, msg_status, parameters, pool_info_read, GovConfig, Parameters,
};
use anchor_basset_reward::msg::HandleMsg::UpdateParams;
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier,
    StdError, StdResult, Storage, WasmMsg,
};
use gov_courier::Deactivated;

#[allow(clippy::too_many_arguments)]
pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_time: Option<u64>,
    underlying_coin_denom: Option<String>,
    undelegated_epoch: Option<u64>,
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
        epoch_time: epoch_time.unwrap_or(params.epoch_time),
        underlying_coin_denom: underlying_coin_denom.unwrap_or(params.underlying_coin_denom),
        undelegated_epoch: undelegated_epoch.unwrap_or(params.undelegated_epoch),
        peg_recovery_fee: peg_recovery_fee.unwrap_or(params.peg_recovery_fee),
        er_threshold: er_threshold.unwrap_or(params.er_threshold),
        reward_denom: reward_denom.clone().unwrap_or(params.reward_denom),
    };

    let mut msgs: Vec<CosmosMsg> = vec![];
    if let Some(denom) = reward_denom {
        let pool = pool_info_read(&deps.storage).load()?;
        let reward_addr = deps.api.human_address(&pool.reward_account)?;

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
                msg_status.burn = Some(msg);
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
    owner: HumanAddr,
) -> StdResult<HandleResponse> {
    let conf = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let owner_raw = deps.api.canonical_address(&owner)?;

    let new_conf = GovConfig { creator: owner_raw };

    config(&mut deps.storage).save(&new_conf)?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![log("action", "change_the_owner")],
        data: None,
    };
    Ok(res)
}
