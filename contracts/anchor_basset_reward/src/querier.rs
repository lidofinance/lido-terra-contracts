use basset::hub::Config;
use cosmwasm_std::{Addr, Binary, CanonicalAddr, Deps, QueryRequest, StdResult, WasmQuery};
use cosmwasm_storage::to_length_prefixed;

pub fn query_token_contract(deps: Deps, contract_addr: Addr) -> StdResult<CanonicalAddr> {
    let conf: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    Ok(conf
        .bluna_token_contract
        .expect("the bLuna token contract must have been registered"))
}

pub fn query_rewards_dispatcher_contract(
    deps: Deps,
    contract_addr: Addr,
) -> StdResult<CanonicalAddr> {
    let conf: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: contract_addr.to_string(),
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    Ok(conf
        .reward_dispatcher_contract
        .expect("the rewards dispatcher contract must have been registered"))
}
