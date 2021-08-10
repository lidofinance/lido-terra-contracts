#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::state::{
    read_airdrop_info, read_all_airdrop_infos, read_config, remove_airdrop_info,
    store_airdrop_info, store_config, update_airdrop_info, Config, CONFIG,
};
use basset::airdrop::{
    ANCAirdropHandleMsg, AirdropInfo, AirdropInfoElem, AirdropInfoResponse, ConfigResponse,
    ExecuteMsg, InstantiateMsg, MIRAirdropHandleMsg, PairHandleMsg, QueryMsg,
};
use basset::hub::ExecuteMsg as HubHandleMsg;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let sndr_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;

    let config = Config {
        owner: sndr_raw,
        hub_contract: msg.hub_contract,
        reward_contract: msg.reward_contract,
        airdrop_tokens: vec![],
    };

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::FabricateMIRClaim {
            stage,
            amount,
            proof,
        } => execute_fabricate_mir_claim(deps, env, info, stage, amount, proof),
        ExecuteMsg::FabricateANCClaim {
            stage,
            amount,
            proof,
        } => execute_fabricate_anchor_claim(deps, env, info, stage, amount, proof),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
            reward_contract,
        } => execute_update_config(deps, env, info, owner, hub_contract, reward_contract),
        ExecuteMsg::AddAirdropInfo {
            airdrop_token,
            airdrop_info,
        } => execute_add_airdrop(deps, env, info, airdrop_token, airdrop_info),
        ExecuteMsg::RemoveAirdropInfo { airdrop_token } => {
            execute_remove_airdrop(deps, env, info, airdrop_token)
        }
        ExecuteMsg::UpdateAirdropInfo {
            airdrop_token,
            airdrop_info,
        } => execute_update_airdrop(deps, env, info, airdrop_token, airdrop_info),
    }
}

fn execute_fabricate_mir_claim(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    stage: u8,
    amount: Uint128,
    proof: Vec<String>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    let mut messages: Vec<SubMsg> = vec![];

    let airdrop_info = read_airdrop_info(deps.storage, "MIR".to_string()).unwrap();
    messages.push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.hub_contract,
        msg: to_binary(&HubHandleMsg::ClaimAirdrop {
            airdrop_token_contract: airdrop_info.airdrop_token_contract,
            airdrop_contract: airdrop_info.airdrop_contract,
            airdrop_swap_contract: airdrop_info.airdrop_swap_contract,
            claim_msg: to_binary(&MIRAirdropHandleMsg::Claim {
                stage,
                amount,
                proof,
            })?,
            swap_msg: to_binary(&PairHandleMsg::Swap {
                belief_price: airdrop_info.swap_belief_price,
                max_spread: airdrop_info.swap_max_spread,
                to: Some(config.reward_contract),
            })?,
        })?,
        funds: vec![],
    })));

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![attr("action", "fabricate_mir_claim")]))
}

fn execute_fabricate_anchor_claim(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    stage: u8,
    amount: Uint128,
    proof: Vec<String>,
) -> StdResult<Response> {
    let config = read_config(deps.storage)?;

    let mut messages: Vec<SubMsg> = vec![];

    let airdrop_info = read_airdrop_info(deps.storage, "ANC".to_string())?;
    messages.push(SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.hub_contract,
        msg: to_binary(&HubHandleMsg::ClaimAirdrop {
            airdrop_token_contract: airdrop_info.airdrop_token_contract,
            airdrop_contract: airdrop_info.airdrop_contract,
            airdrop_swap_contract: airdrop_info.airdrop_swap_contract,
            claim_msg: to_binary(&ANCAirdropHandleMsg::Claim {
                stage,
                amount,
                proof,
            })?,
            swap_msg: to_binary(&PairHandleMsg::Swap {
                belief_price: airdrop_info.swap_belief_price,
                max_spread: airdrop_info.swap_max_spread,
                to: Some(config.reward_contract),
            })?,
        })?,
        funds: vec![],
    })));

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(vec![attr("action", "fabricate_anc_claim")]))
}
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
    reward_contract: Option<String>,
) -> StdResult<Response> {
    // only owner can send this message.
    let mut config = read_config(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(&o)?;
        config.owner = owner_raw
    }
    if let Some(hub) = hub_contract {
        config.hub_contract = hub;
    }
    if let Some(reward_addr) = reward_contract {
        config.reward_contract = reward_addr;
    }

    store_config(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_config")]))
}

pub fn execute_add_airdrop(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<Response> {
    // only owner can send this message.
    let config = read_config(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    let exists = read_airdrop_info(deps.storage, airdrop_token.clone());
    if exists.is_ok() {
        return Err(StdError::generic_err(format!(
            "There is a token info with this {}",
            airdrop_token
        )));
    }

    CONFIG.update(deps.storage, |mut conf| -> StdResult<Config> {
        conf.airdrop_tokens.push(airdrop_token.clone());
        Ok(conf)
    })?;

    store_airdrop_info(deps.storage, airdrop_token.clone(), airdrop_info)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "add_airdrop_info"),
        attr("airdrop_token", airdrop_token),
    ]))
}

pub fn execute_update_airdrop(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<Response> {
    // only owner can send this message.
    let config = read_config(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    let exists = read_airdrop_info(deps.storage, airdrop_token.clone());
    if exists.is_err() {
        return Err(StdError::generic_err(format!(
            "There is no token info with this {}",
            airdrop_token
        )));
    }

    update_airdrop_info(deps.storage, airdrop_token.clone(), airdrop_info)?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "update_airdrop_info"),
        attr("airdrop_token", airdrop_token),
    ]))
}

pub fn execute_remove_airdrop(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    airdrop_token: String,
) -> StdResult<Response> {
    // only owner can send this message.
    let config = read_config(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(&info.sender.to_string())?;
    if sender_raw != config.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    let exists = read_airdrop_info(deps.storage, airdrop_token.clone());
    if exists.is_err() {
        return Err(StdError::generic_err(format!(
            "There is no token info with this {}",
            airdrop_token
        )));
    }

    CONFIG.update(deps.storage, |mut conf| -> StdResult<Config> {
        conf.airdrop_tokens.retain(|item| item != &airdrop_token);
        Ok(conf)
    })?;

    remove_airdrop_info(deps.storage, airdrop_token.clone())?;
    Ok(Response::new().add_attributes(vec![
        attr("action", "remove_airdrop_info"),
        attr("airdrop_token", airdrop_token),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::AirdropInfo {
            airdrop_token,
            start_after,
            limit,
        } => to_binary(&query_airdrop_infos(
            deps,
            airdrop_token,
            start_after,
            limit,
        )?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = read_config(deps.storage)?;
    let owner_addr = deps.api.addr_humanize(&config.owner)?;

    Ok(ConfigResponse {
        owner: owner_addr.to_string(),
        hub_contract: config.hub_contract,
        reward_contract: config.reward_contract,
        airdrop_tokens: config.airdrop_tokens,
    })
}

fn query_airdrop_infos(
    deps: Deps,
    airdrop_token: Option<String>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AirdropInfoResponse> {
    if let Some(air_token) = airdrop_token {
        let info = read_airdrop_info(deps.storage, air_token.clone()).unwrap();

        Ok(AirdropInfoResponse {
            airdrop_info: vec![AirdropInfoElem {
                airdrop_token: air_token,
                info,
            }],
        })
    } else {
        let infos = read_all_airdrop_infos(deps.storage, start_after, limit)?;
        Ok(AirdropInfoResponse {
            airdrop_info: infos,
        })
    }
}
