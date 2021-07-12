use cosmwasm_std::{
    to_binary, Api, Binary, Coin, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};

use crate::common::calculate_delegations;
use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::registry::{
    config, config_read, registry, registry_read, store_config, Config, Validator,
};
use hub_querier::HandleMsg::{RedelegateProxy, UpdateGlobalIndex};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    config(&mut deps.storage).save(&Config {
        owner: deps.api.canonical_address(&env.message.sender)?,
        hub_contract: deps.api.canonical_address(&msg.hub_contract)?,
    })?;

    for v in msg.registry {
        registry(&mut deps.storage).save(v.address.as_str().as_bytes(), &v)?;
    }

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::AddValidator { validator } => add_validator(deps, env, validator),
        HandleMsg::RemoveValidator { address } => remove_validator(deps, env, address),
        HandleMsg::UpdateConfig {
            owner,
            hub_contract,
        } => handle_update_config(deps, env, owner, hub_contract),
    }
}

/// Update the config. Update the owner and hub contract address.
/// Only creator/owner is allowed to execute
pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    hub_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    // only owner must be able to send this message.
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.owner = owner_raw;
            Ok(last_config)
        })?;
    }

    if let Some(hub) = hub_contract {
        let hub_raw = deps.api.canonical_address(&hub)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.hub_contract = hub_raw;
            Ok(last_config)
        })?;
    }

    Ok(HandleResponse::default())
}

pub fn add_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: Validator,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    registry(&mut deps.storage).save(validator.address.as_str().as_bytes(), &validator)?;
    Ok(HandleResponse::default())
}

pub fn remove_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator_address: HumanAddr,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    registry(&mut deps.storage).remove(validator_address.as_str().as_bytes());

    let config = config_read(&deps.storage).load()?;
    let hub_address = deps.api.human_address(&config.hub_contract)?;

    let query = deps
        .querier
        .query_delegation(hub_address.clone(), validator_address.clone());

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Ok(q) = query {
        let delegated_amount = q;
        let mut validators = query_validators(deps)?;
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
                    contract_addr: hub_address.clone(),
                    msg: to_binary(&regelegate_msg)?,
                    send: vec![],
                }));
            }

            let msg = UpdateGlobalIndex {
                airdrop_hooks: None,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: hub_address,
                msg: to_binary(&msg)?,
                send: vec![],
            }));
        }
    }

    let res = HandleResponse {
        messages,
        data: None,
        log: vec![],
    };

    Ok(res)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetValidatorsForDelegation {} => {
            let mut validators = query_validators(deps)?;
            validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));
            to_binary(&validators)
        }
    }
}

fn query_validators<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Vec<Validator>> {
    let config = config_read(&deps.storage).load()?;
    let hub_address = deps.api.human_address(&config.hub_contract)?;

    let mut validators: Vec<Validator> = vec![];
    let registry = registry_read(&deps.storage);
    for item in registry.range(None, None, cosmwasm_std::Order::Ascending) {
        let mut validator = Validator {
            total_delegated: Default::default(),
            address: item?.1.address,
        };
        let delegation = deps
            .querier
            .query_delegation(&hub_address, &validator.address)
            .unwrap_or_default();
        validator.total_delegated = if let Some(d) = delegation {
            d.amount.amount
        } else {
            Uint128::zero()
        };
        validators.push(validator);
    }
    Ok(validators)
}
