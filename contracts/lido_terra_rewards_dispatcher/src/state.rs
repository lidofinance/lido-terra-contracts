// Copyright 2021 Lido
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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Addr, CanonicalAddr, Decimal};

use cw_storage_plus::Item;

pub static CONFIG: Item<Config> = Item::new("config");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: CanonicalAddr,
    pub bluna_reward_contract: CanonicalAddr,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
    pub lido_fee_address: CanonicalAddr,
    pub lido_fee_rate: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: Addr,
    pub hub_contract: Addr,
    pub bluna_reward_contract: Addr,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
    pub lido_fee_address: Addr,
    pub lido_fee_rate: Decimal,
}
