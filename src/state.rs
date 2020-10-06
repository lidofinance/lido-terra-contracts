use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{ReadonlyStorage, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, ReadonlySingleton,
    Singleton,
};

pub static TOKEN_INFO_KEY: &[u8] = b"token_info";
const BALANCE: &[u8] = b"balance";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

pub fn token_info<S: Storage>(storage: &mut S) -> Singleton<S, TokenInfo> {
    singleton(storage, TOKEN_INFO_KEY)
}

pub fn token_info_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, TokenInfo> {
    singleton_read(storage, TOKEN_INFO_KEY)
}

pub fn balances<S: Storage>(storage: &mut S) -> Bucket<S, Uint128> {
    bucket(BALANCE, storage)
}

pub fn balances_read<S: ReadonlyStorage>(storage: &S) -> ReadonlyBucket<S, Uint128> {
    bucket_read(BALANCE, storage)
}
