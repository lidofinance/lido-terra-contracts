use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::CanonicalAddr;
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};

pub static CONFIG: Item<Config> = Item::new("config");

pub static REGISTRY: Map<&[u8], Validator> = Map::new("validators_registry");

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: CanonicalAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Validator {
    #[serde(default)]
    pub total_delegated: Uint128,

    pub address: String,
}
