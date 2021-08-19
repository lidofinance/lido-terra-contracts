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
