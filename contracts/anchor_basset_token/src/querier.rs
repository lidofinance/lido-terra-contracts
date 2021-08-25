use cosmwasm_std::{Addr, Binary, DepsMut, QueryRequest, StdError, StdResult, WasmQuery};
use cosmwasm_storage::to_length_prefixed;

use crate::state::read_hub_contract;
use anchor_basset_rewards_dispatcher::state::Config as RewardsDispatcherConfig;
use basset::hub::Config;

pub fn query_reward_contract(deps: &DepsMut) -> StdResult<Addr> {
    let hub_address = deps.api.addr_humanize(&read_hub_contract(deps.storage)?)?;

    let config: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address.to_string(),
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    let rewards_dispatcher_address =
        deps.api
            .addr_humanize(&config.reward_dispatcher_contract.ok_or_else(|| {
                StdError::generic_err("the rewards dispatcher contract must have been registered")
            })?)?;

    let rewards_dispatcher_config: RewardsDispatcherConfig =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
            contract_addr: rewards_dispatcher_address.to_string(),
            key: Binary::from(to_length_prefixed(b"config")),
        }))?;

    let bluna_reward_address = deps
        .api
        .addr_humanize(&rewards_dispatcher_config.bluna_reward_contract)?;

    Ok(bluna_reward_address)
}
