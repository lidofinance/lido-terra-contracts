// Copyright 2021 Anchor Protocol. Modified by Lido
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_slice, to_binary, to_vec, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult,
};

use crate::state::{
    read_airdrop_info, read_all_airdrop_infos, read_config, remove_airdrop_info,
    store_airdrop_info, store_config, update_airdrop_info, Config, AIRDROP_INFO, CONFIG,
};
use basset::airdrop::{
    AirdropInfo, AirdropInfoElem, AirdropInfoResponse, ConfigResponse, ExecuteMsg, InstantiateMsg,
    MigrateMsg, QueryMsg,
};

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
        airdrop_tokens: vec![],
    };

    store_config(deps.storage, &config)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
        } => execute_update_config(deps, env, info, owner, hub_contract),
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

pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
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
        let info = read_airdrop_info(deps.storage, air_token.clone())?;

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    let limit = 10_usize;

    let infos: StdResult<Vec<AirdropInfoElem>> = AIRDROP_INFO
        .range(deps.storage, None, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (k, v) = item?;
            let key: String = from_slice(&k)?;
            Ok(AirdropInfoElem {
                airdrop_token: key,
                info: v,
            })
        })
        .collect();

    for info_elem_to_rewrite in infos? {
        let key = to_vec(&info_elem_to_rewrite.airdrop_token)?;
        AIRDROP_INFO.save(
            deps.storage,
            &key,
            &AirdropInfo {
                airdrop_token_contract: info_elem_to_rewrite.info.airdrop_token_contract,
                airdrop_contract: info_elem_to_rewrite.info.airdrop_contract,
            },
        )?;
    }

    let config_to_rewrite = CONFIG.load(deps.storage)?;
    CONFIG.save(
        deps.storage,
        &Config {
            owner: config_to_rewrite.owner,
            hub_contract: config_to_rewrite.hub_contract,
            airdrop_tokens: config_to_rewrite.airdrop_tokens,
        },
    )?;

    Ok(Response::new())
}
