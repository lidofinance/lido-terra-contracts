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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use basset::airdrop::{AirdropInfo, AirdropInfoElem};
use cosmwasm_std::{from_slice, to_vec, CanonicalAddr, Order, StdResult, Storage};
use cw_storage_plus::{Bound, Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: String,
    pub reward_contract: String,
    pub airdrop_tokens: Vec<String>,
}

pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");
pub const AIRDROP_INFO: Map<&[u8], AirdropInfo> = Map::new("airdrop_info");

pub fn store_config(storage: &mut dyn Storage, config: &Config) -> StdResult<()> {
    CONFIG.save(storage, config)
}

pub fn read_config(storage: &dyn Storage) -> StdResult<Config> {
    CONFIG.load(storage)
}

pub fn store_airdrop_info(
    storage: &mut dyn Storage,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    AIRDROP_INFO.save(storage, &key, &airdrop_info)
}

pub fn update_airdrop_info(
    storage: &mut dyn Storage,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    AIRDROP_INFO.update(storage, &key, |_| -> StdResult<_> { Ok(airdrop_info) })?;
    Ok(())
}

pub fn remove_airdrop_info(storage: &mut dyn Storage, airdrop_token: String) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    AIRDROP_INFO.remove(storage, &key);
    Ok(())
}

pub fn read_airdrop_info(storage: &dyn Storage, airdrop_token: String) -> StdResult<AirdropInfo> {
    let key = to_vec(&airdrop_token)?;
    AIRDROP_INFO.load(storage, &key)
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_all_airdrop_infos(
    storage: &dyn Storage,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<AirdropInfoElem>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after)?.map(Bound::exclusive);

    let infos: StdResult<Vec<AirdropInfoElem>> = AIRDROP_INFO
        .range(storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (k, v) = match item {
                Ok(p) => p,
                Err(e) => return Err(e),
            };
            let key: String = match from_slice(&k) {
                Ok(s) => s,
                Err(e) => return Err(e),
            };
            Ok(AirdropInfoElem {
                airdrop_token: key,
                info: v,
            })
        })
        .collect();

    infos
}

fn calc_range_start(start_after: Option<String>) -> StdResult<Option<Vec<u8>>> {
    if let Some(air) = start_after {
        let mut v = to_vec(&air)?;
        v.push(1);
        return Ok(Some(v));
    }
    Ok(None)
}
