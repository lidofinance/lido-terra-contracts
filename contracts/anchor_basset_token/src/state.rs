use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
//use cosmwasm_storage::{singleton, singleton_read};
use cw_storage_plus::Item;

pub const HUB_CONTRACT_KEY: Item<CanonicalAddr> = Item::new("\u{0}\nhub_contract");

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
