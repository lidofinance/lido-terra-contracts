use cosmwasm_std::{
    entry_point, to_binary, Binary, Coin, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, WasmMsg,
};

use crate::common::calculate_delegations;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::registry::{
    config, config_read, registry, registry_read, store_config, Config, Validator,
};
use hub_querier::HandleMsg::{RedelegateProxy, UpdateGlobalIndex};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    config(deps.storage).save(&Config {
        owner: deps.api.addr_canonicalize(&info.sender.as_str())?,
        hub_contract: deps.api.addr_canonicalize(&msg.hub_contract.as_str())?,
    })?;

    for v in msg.registry {
        registry(deps.storage).save(v.address.as_str().as_bytes(), &v)?;
    }

    Ok(Response::default())
}

#[entry_point]
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
    let config = config_read(deps.storage).load()?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    if info.sender != owner_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(&o)?;

        store_config(deps.storage).update(|mut last_config| -> StdResult<_> {
            last_config.owner = owner_raw;
            Ok(last_config)
        })?;
    }

    if let Some(hub) = hub_contract {
        let hub_raw = deps.api.addr_canonicalize(&hub)?;

        store_config(deps.storage).update(|mut last_config| -> StdResult<_> {
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
    let config = config_read(deps.storage).load()?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;
    if info.sender != owner_address && info.sender != hub_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    registry(deps.storage).save(validator.address.as_str().as_bytes(), &validator)?;
    Ok(Response::default())
}

pub fn remove_validator(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    validator_address: String,
) -> StdResult<Response> {
    let config = config_read(deps.storage).load()?;
    let owner_address = deps.api.addr_humanize(&config.owner)?;
    if info.sender != owner_address {
        return Err(StdError::generic_err("unauthorized"));
    }

    let validators_number = registry(deps.storage)
        .range(None, None, cosmwasm_std::Order::Ascending)
        .count();

    if validators_number == 1 {
        return Err(StdError::generic_err(
            "Cannot remove the last validator in the registry",
        ));
    }

    registry(deps.storage).remove(validator_address.as_str().as_bytes());

    let config = config_read(deps.storage).load()?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;

    let query = deps
        .querier
        .query_delegation(hub_address.clone(), validator_address.clone());

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Ok(q) = query {
        let delegated_amount = q;
        let mut validators = query_validators(deps.as_ref())?;
        validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

        if let Some(delegation) = delegated_amount {
            let (_, delegations) =
                calculate_delegations(delegation.amount.amount, validators.as_slice())?;

            for i in 0..delegations.len() {
                if delegations[i].is_zero() {
                    continue;
                }
                let regelegate_msg = RedelegateProxy {
                    src_validator: validator_address.clone(),
                    dst_validator: validators[i].address.clone(),
                    amount: Coin::new(delegations[i].u128(), delegation.amount.denom.as_str()),
                };
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: hub_address.clone().into_string(),
                    msg: to_binary(&regelegate_msg)?,
                    funds: vec![],
                }));
            }

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

#[entry_point]
pub fn query(deps: Deps, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetValidatorsForDelegation {} => {
            let mut validators = query_validators(deps)?;
            validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));
            to_binary(&validators)
        }
    }
}

fn query_validators(deps: Deps) -> StdResult<Vec<Validator>> {
    let config = config_read(deps.storage).load()?;
    let hub_address = deps.api.addr_humanize(&config.hub_contract)?;

    let delegations = deps.querier.query_all_delegations(&hub_address)?;

    let mut validators: Vec<Validator> = vec![];
    let registry = registry_read(deps.storage);
    for item in registry.range(None, None, cosmwasm_std::Order::Ascending) {
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
