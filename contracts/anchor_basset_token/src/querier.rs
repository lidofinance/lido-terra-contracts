use cosmwasm_std::{
    from_binary, Api, Binary, Extern, HumanAddr, Querier, QueryRequest, StdResult, Storage,
    WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;

use crate::state::read_hub_contract;
use hub_courier::PoolInfo;

pub fn query_reward_contract<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<HumanAddr> {
    let hub_address = deps
        .api
        .human_address(&read_hub_contract(&deps.storage).unwrap())
        .unwrap();

    let res: Binary = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Raw {
        contract_addr: hub_address,
        key: Binary::from(to_length_prefixed(b"pool_info")),
    }))?;

    let pool_info: PoolInfo = from_binary(&res)?;
    let address = deps.api.human_address(&pool_info.reward_account).unwrap();
    Ok(address)
}
