// Copyright 2021 Lido
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//     http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    to_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128, WasmMsg,
};

use crate::common::calculate_delegations;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::registry::{Config, Validator, CONFIG, REGISTRY};
use basset::hub::ExecuteMsg::{RedelegateProxy, UpdateGlobalIndex};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_canonicalize(info.sender.as_str())?,
            hub_contract: deps.api.addr_canonicalize(msg.hub_contract.as_str())?,
        },
    )?;

    for v in msg.registry {
        REGISTRY.save(deps.storage, v.address.as_str().as_bytes(), &v)?;
    }

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(deps: DepsMut, env: Env, info: MessageInfo, msg: ExecuteMsg) -> StdResult<Response> {
    match msg {
        ExecuteMsg::AddValidator { validator } => add_validator(deps, env, info, validator),
        ExecuteMsg::RemoveValidator { address } => remove_validator(deps, env, info, address),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
        } => execute_update_config(deps, env, info, owner, hub_contract),
    }
}

/// Update the config. Update the owner and hub contract address.
/// Only creator/owner is allowed to execute
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
) -> StdResult<Response> {
    // only owner must be able to send this message.
    let config = CONFIG.load(deps.storage)?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    if info.sender != owner_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(&o)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.owner = owner_raw;
            Ok(last_config)
        })?;
    }

    if let Some(hub) = hub_contract {
        let hub_raw = deps.api.addr_canonicalize(&hub)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.hub_contract = hub_raw;
            Ok(last_config)
        })?;
    }

    Ok(Response::default())
}

pub fn add_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator: Validator,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;
    if info.sender != owner_address && info.sender != hub_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    REGISTRY.save(
        deps.storage,
        validator.address.as_str().as_bytes(),
        &validator,
    )?;
    Ok(Response::default())
}

pub fn remove_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator_address: String,
) -> StdResult<Response> {
    let config = CONFIG.load(deps.storage)?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    if info.sender != owner_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let validators_number = REGISTRY
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();

    if validators_number == 1 {
        return Err(StdError::generic_err(
            "Cannot remove the last validator in the registry",
        ));
    }

    REGISTRY.remove(deps.storage, validator_address.as_str().as_bytes());

    let config = CONFIG.load(deps.storage)?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;

    let query = deps
        .querier
        .query_delegation(hub_address.clone(), validator_address.clone());

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Ok(q) = query {
        let delegated_amount = q;
        let mut validators = query_validators(deps.as_ref())?;
        validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

        let mut redelegations: Vec<(String, Coin)> = vec![];
        if let Some(delegation) = delegated_amount {
            // Terra core returns zero if there is another active redelegation
            // That means we cannot start a new redelegation, so we only remove a validator from
            // the registry.
            // We'll do a redelegation manually later by sending RedelegateProxy to the hub
            if delegation.can_redelegate.amount < delegation.amount.amount {
                return StdResult::Ok(Response::new());
            }

            let (_, delegations) =
                calculate_delegations(delegation.amount.amount, validators.as_slice())?;

            for i in 0..delegations.len() {
                if delegations[i].is_zero() {
                    continue;
                }
                redelegations.push((
                    validators[i].address.clone(),
                    Coin::new(delegations[i].u128(), delegation.amount.denom.as_str()),
                ));
            }

            let regelegate_msg = RedelegateProxy {
                src_validator: validator_address,
                redelegations,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: hub_address.clone().into_string(),
                msg: to_binary(&regelegate_msg)?,
                funds: vec![],
            }));

            let msg = UpdateGlobalIndex {
                airdrop_hooks: None,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: hub_address.into_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            }));
        }
    }

    let res = Response::new().add_messages(messages);
    Ok(res)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetValidatorsForDelegation {} => {
            let mut validators = query_validators(deps)?;
            validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));
            to_binary(&validators)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

fn query_validators(deps: Deps) -> StdResult<Vec<Validator>> {
    let config = CONFIG.load(deps.storage)?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;

    let delegations = deps.querier.query_all_delegations(&hub_address)?;

    let mut validators: Vec<Validator> = vec![];
    for item in REGISTRY.range(deps.storage, None, None, cosmwasm_std::Order::Ascending) {
        let mut validator = Validator {
            total_delegated: Default::default(),
            address: item?.1.address,
        };
        // There is a bug in terra/core.
        // The bug happens when we do query_delegation() but there are no delegation pair (delegator-validator)
        // but query_delegation() fails with a parse error cause terra/core returns an empty FullDelegation struct
        // instead of a nil pointer to the struct.
        // https://github.com/terra-money/core/blob/58602320d2907814cfccdf43e9679468bb4bd8d3/x/staking/wasm/interface.go#L227
        // So we do query_all_delegations() instead of query_delegation().unwrap()
        // and try to find delegation in the returned vec
        validator.total_delegated = if let Some(d) = delegations
            .iter()
            .find(|d| d.validator == validator.address)
        {
            d.amount.amount
        } else {
            Uint128::zero()
        };
        validators.push(validator);
    }
    Ok(validators)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
