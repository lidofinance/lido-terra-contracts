use cosmwasm_std::{CanonicalAddr, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read};

const HUB_CONTRACT_KEY: &[u8] = b"hub_contract";

// meta is the token definition as well as the total_supply
pub fn read_hub_contract(storage: &dyn Storage) -> StdResult<CanonicalAddr> {
    singleton_read(storage, HUB_CONTRACT_KEY).load()
}

pub fn store_hub_contract(
    storage: &mut dyn Storage,
    hub_contract: &CanonicalAddr,
) -> StdResult<()> {
    singleton(storage, HUB_CONTRACT_KEY).save(hub_contract)
}
