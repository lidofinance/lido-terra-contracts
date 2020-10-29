use cosmwasm_std::{CanonicalAddr, Decimal, ReadonlyStorage, Storage};
use cosmwasm_storage::{singleton, singleton_read, ReadonlySingleton, Singleton};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub static CONFIG: &[u8] = b"config";
pub static INDEX: &[u8] = b"index";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, CONFIG)
}

pub fn config_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Config> {
    singleton_read(storage, CONFIG)
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Index {
    pub global_index: Decimal,
}

pub fn index_store<S: Storage>(storage: &mut S) -> Singleton<S, Index> {
    singleton(storage, INDEX)
}

pub fn index_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Index> {
    singleton_read(storage, INDEX)
}
