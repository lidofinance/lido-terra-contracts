#![allow(clippy::field_reassign_with_default)] //https://github.com/CosmWasm/cosmwasm/issues/685

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::Storage;
use cosmwasm_std::{HumanAddr, Uint128};
use cosmwasm_storage::{bucket, bucket_read, Bucket, ReadonlyBucket};

pub static CONFIG_KEY: &[u8] = b"config";

pub static REGISTRY_KEY: &[u8] = b"validators_registry";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Validator {
    pub active: bool,

    #[serde(default)]
    pub total_delegated: Uint128,

    pub address: HumanAddr,
}

pub fn registry<S: Storage>(storage: &mut S) -> Bucket<S, Validator> {
    bucket(REGISTRY_KEY, storage)
}

pub fn registry_read<S: Storage>(storage: &S) -> ReadonlyBucket<S, Validator> {
    bucket_read(REGISTRY_KEY, storage)
}
