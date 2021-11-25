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

use cosmwasm_std::{to_binary, Addr, DepsMut, QueryRequest, StdError, StdResult, WasmQuery};

use crate::state::read_hub_contract;
use basset::hub::{ConfigResponse, QueryMsg as HubQueryMsg};
use lido_terra_rewards_dispatcher::msg::QueryMsg as RewardsDispatcherQueryMsg;
use lido_terra_rewards_dispatcher::state::Config as RewardsDispatcherConfig;

pub fn query_reward_contract(deps: &DepsMut) -> StdResult<Addr> {
    let hub_address = deps.api.addr_humanize(&read_hub_contract(deps.storage)?)?;

    let config: ConfigResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: hub_address.to_string(),
        msg: to_binary(&HubQueryMsg::Config {})?,
    }))?;

    let rewards_dispatcher_address = deps.api.addr_humanize(
        &deps.api.addr_canonicalize(
            config
                .reward_dispatcher_contract
                .ok_or_else(|| {
                    StdError::generic_err(
                        "the rewards dispatcher contract must have been registered",
                    )
                })?
                .as_str(),
        )?,
    )?;

    let rewards_dispatcher_config: RewardsDispatcherConfig =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: rewards_dispatcher_address.to_string(),
            msg: to_binary(&RewardsDispatcherQueryMsg::Config {})?,
        }))?;

    let bluna_reward_address = deps
        .api
        .addr_humanize(&rewards_dispatcher_config.bluna_reward_contract)?;

    Ok(bluna_reward_address)
}
