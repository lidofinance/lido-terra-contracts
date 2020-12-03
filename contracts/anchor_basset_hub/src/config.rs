use crate::state::{config_read, msg_status, parameter, pool_info_read, Parameters};
use anchor_basset_reward::msg::HandleMsg::UpdateParams;
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, Querier, StdError,
    StdResult, Storage, WasmMsg,
};
use gov_courier::Deactivated;

#[allow(clippy::too_many_arguments)]

pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_time: u64,
    underlying_coin_denom: String,
    undelegated_epoch: u64,
    peg_recovery_fee: Decimal,
    er_threshold: Decimal,
    swap_denom: Option<String>,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }
    let params = Parameters {
        epoch_time,
        underlying_coin_denom,
        undelegated_epoch,
        peg_recovery_fee,
        er_threshold,
    };

    let mut msgs: Vec<CosmosMsg> = vec![];
    if let Some(denom) = swap_denom {
        let pool = pool_info_read(&deps.storage).load()?;
        let reward_addr = deps.api.human_address(&pool.reward_account)?;

        //send update params to the reward contract
        let set_swap = UpdateParams { swap_denom: denom };
        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_addr,
            msg: to_binary(&set_swap)?,
            send: vec![],
        }));
    }

    parameter(&mut deps.storage).save(&params)?;
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
