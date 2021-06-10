use cosmwasm_std::{Addr, Binary, DepsMut, QueryRequest, StdResult, WasmQuery};
use cosmwasm_storage::to_length_prefixed;

use crate::state::read_hub_contract;
use basset::hub::Config;

pub fn query_reward_contract(deps: &DepsMut) -> StdResult<Addr> {
    let hub_address = deps
        .api
        .addr_humanize(&read_hub_contract(deps.storage).unwrap())
        .unwrap();

    let config: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address.to_string(),
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    let address = deps
        .api
        .addr_humanize(
            &config
                .reward_contract
                .expect("the reward contract must have been registered"),
        )
        .unwrap();
    Ok(address)
}
