use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{CanonicalAddr, ReadonlyStorage, StdResult, Storage};
use cosmwasm_storage::{singleton, singleton_read};

pub static KEY_CONFIG: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub hub_contract: CanonicalAddr,
    pub bluna_reward_contract: CanonicalAddr,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
}

pub fn store_config<S: Storage>(storage: &mut S, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config<S: ReadonlyStorage>(storage: &S) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}
