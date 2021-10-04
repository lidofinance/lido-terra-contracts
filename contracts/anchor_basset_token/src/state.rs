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

use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
//use cosmwasm_storage::{singleton, singleton_read};
use cw_storage_plus::Item;

pub const HUB_CONTRACT_KEY: Item<CanonicalAddr> = Item::new("\u{0}\u{c}hub_contract");

// meta is the token definition as well as the total_supply
pub fn read_hub_contract(storage: &dyn Storage) -> StdResult<CanonicalAddr> {
    HUB_CONTRACT_KEY.load(storage)
}

pub fn store_hub_contract(
    storage: &mut dyn Storage,
    hub_contract: &CanonicalAddr,
) -> StdResult<()> {
    HUB_CONTRACT_KEY.save(storage, hub_contract)
}

#[cfg(test)]
mod test {
    use super::*;

    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{Api, StdResult, Storage};
    use cosmwasm_storage::{singleton, singleton_read};

    pub static HUB_CONTRACT: &[u8] = b"hub_contract";

    pub fn store_hub(storage: &mut dyn Storage, params: &CanonicalAddr) -> StdResult<()> {
        singleton(storage, HUB_CONTRACT).save(params)
    }
    pub fn read_hub(storage: &dyn Storage) -> StdResult<CanonicalAddr> {
        singleton_read(storage, HUB_CONTRACT).load()
    }

    #[test]
    fn hub_legacy_compatibility() {
        let mut deps = mock_dependencies(&[]);
        store_hub(
            &mut deps.storage,
            &deps.api.addr_canonicalize("hub").unwrap(),
        )
        .unwrap();

        assert_eq!(
            HUB_CONTRACT_KEY.load(&deps.storage).unwrap(),
            read_hub(&deps.storage).unwrap()
        );
    }
}
