use crate::state::{config_read, msg_status, parameter, Parameters};
use cosmwasm_std::{log, Api, Env, Extern, HandleResponse, Querier, StdError, StdResult, Storage};
use gov_courier::Deactivated;

pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_time: u64,
    coin_denom: String,
    undelegated_epoch: u64,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }
    let params = Parameters {
        epoch_time,
        supported_coin_denom: coin_denom,
        undelegated_epoch,
    };

    parameter(&mut deps.storage).save(&params)?;
    let res = HandleResponse {
        messages: vec![],
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
        Deactivated::Burn => {
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
