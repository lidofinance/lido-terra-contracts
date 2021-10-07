// Copyright 2021 Anchor Protocol. Modified by Lido
//
// Licensedicensed under the Apache License, Version 2.0 (the "License");
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

use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub hub_contract: String,
    pub bluna_reward_contract: String,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
    pub lido_fee_address: String,
    pub lido_fee_rate: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SwapToRewardDenom {
        bluna_total_mint_amount: Uint128,
        stluna_total_mint_amount: Uint128,
    },
    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
        bluna_reward_contract: Option<String>,
        stluna_reward_denom: Option<String>,
        bluna_reward_denom: Option<String>,
        lido_fee_address: Option<String>,
        lido_fee_rate: Option<Decimal>,
    },
    DispatchRewards {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetBufferedRewards returns the buffered amount of bLuna and stLuna rewards.
    GetBufferedRewards {},
    // Config returns config
    Config {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetBufferedRewardsResponse {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
