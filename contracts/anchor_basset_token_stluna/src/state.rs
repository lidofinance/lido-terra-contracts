use cosmwasm_std::CanonicalAddr;
use cw_storage_plus::Item;

pub const HUB_CONTRACT: Item<CanonicalAddr> = Item::new("hub_contract");
