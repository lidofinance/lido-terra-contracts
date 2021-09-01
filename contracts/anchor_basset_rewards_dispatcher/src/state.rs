use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, Decimal};

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
