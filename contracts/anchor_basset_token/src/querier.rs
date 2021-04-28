use cosmwasm_std::{
    from_binary, Api, Binary, Decimal, Extern, HumanAddr, Querier, QueryRequest, StdResult,
    Storage, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;

use crate::state::read_hub_contract;

use anchor_basset_rewards_dispatcher::state::Config as RewardsDispatcherConfig;

use hub_querier::{Config as HubConfig, State};


pub fn query_reward_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<HumanAddr> {
    let hub_address = deps
        .api
        .human_address(&read_hub_contract(&deps.storage).unwrap())
        .unwrap();

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address,
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    let hub_config: HubConfig = from_binary(&res)?;
    let rewards_dispatcher_address = deps
        .api
        .human_address(
            &hub_config
                .reward_dispatcher_contract
                .expect("the rewards dispatcher contract must have been registered"),
        )
        .unwrap();

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: rewards_dispatcher_address,
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    let rewards_dispatcher_config: RewardsDispatcherConfig = from_binary(&res)?;
    let rewards_address = deps
        .api
        .human_address(&rewards_dispatcher_config.bluna_reward_contract)
        .unwrap();
    Ok(rewards_address)
}

pub fn query_exchange_rates<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<(Decimal, Decimal)> {
    let hub_address = deps
        .api
        .human_address(&read_hub_contract(&deps.storage).unwrap())
        .unwrap();

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address,
        key: Binary::from(to_length_prefixed(b"state")),
    }))?;

    let config: State = from_binary(&res)?;

    Ok((config.bluna_exchange_rate, config.stluna_exchange_rate))
}

pub fn query_stluna_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<HumanAddr> {
    let hub_address = deps
        .api
        .human_address(&read_hub_contract(&deps.storage).unwrap())
        .unwrap();

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address,
        key: Binary::from(to_length_prefixed(b"config")),
    }))?;

    let config: HubConfig = from_binary(&res)?;
    let address = deps
        .api
        .human_address(
            &config
                .stluna_token_contract
                .expect("the stluna token contract must have been registered"),
        )
        .unwrap();
    Ok(address)
}
