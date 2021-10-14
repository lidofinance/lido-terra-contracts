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

use basset::hub::{Config, QueryMsg};
use cosmwasm_std::{
    to_binary, Addr, CanonicalAddr, Deps, QueryRequest, StdError, StdResult, WasmQuery,
};

pub fn query_token_contract_address(
    deps: Deps,
    hub_contract_addr: Addr,
) -> StdResult<CanonicalAddr> {
    let conf: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: hub_contract_addr.to_string(),
        msg: to_binary(&QueryMsg::Config {})?,
    }))?;

    conf.bluna_token_contract
        .ok_or_else(|| StdError::generic_err("the bLuna token contract must have been registered"))
}

pub fn query_rewards_dispatcher_contract_address(
    deps: Deps,
    hub_contract_addr: Addr,
) -> StdResult<CanonicalAddr> {
    let conf: Config = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: hub_contract_addr.to_string(),
        msg: to_binary(&QueryMsg::Config {})?,
    }))?;

    conf.reward_dispatcher_contract.ok_or_else(|| {
        StdError::generic_err("the rewards dispatcher contract must have been registered")
    })
}
