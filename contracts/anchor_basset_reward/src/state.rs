use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, Decimal, HumanAddr, Order, ReadonlyStorage, StdError,
    StdResult, Storage, Uint128,
};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, PrefixedStorage, ReadonlyBucket,
    ReadonlyPrefixedStorage, ReadonlySingleton, Singleton,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub static CONFIG: &[u8] = b"config";
pub static INDEX: &[u8] = b"index";
pub static PREFIX_HOLDERS_MAP: &[u8] = b"holders";
static PENDING_REWARD: &[u8] = b"pending_reward";

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

// This is similar to HashMap<holder's address, reward_index>
pub fn store_holder_map<S: Storage>(
    storage: &mut S,
    holder_address: HumanAddr,
    index: Decimal,
) -> StdResult<()> {
    let vec = to_vec(&holder_address)?;
    let value: Vec<u8> = to_vec(&index)?;
    PrefixedStorage::new(PREFIX_HOLDERS_MAP, storage).set(&vec, &value);
    Ok(())
}

pub fn read_holder_map<S: Storage>(storage: &S, holder_address: HumanAddr) -> StdResult<Decimal> {
    let vec = to_vec(&holder_address)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_HOLDERS_MAP, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err("no holder is found")),
    }
}

// Returns a HashMap of holders. <holders, reward_index>
pub fn read_holders<S: Storage>(storage: &S) -> StdResult<HashMap<HumanAddr, Decimal>> {
    let mut holders: HashMap<HumanAddr, Decimal> = HashMap::new();
    let res = ReadonlyPrefixedStorage::new(PREFIX_HOLDERS_MAP, storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, value) = item;
            let sender = from_slice(&key).unwrap();
            let index: Decimal = from_slice(&value).unwrap();
            holders.insert(sender, index);
        })
        .collect();

    Ok(holders)
}

pub fn pending_reward_store<S: Storage>(storage: &mut S) -> Bucket<S, Uint128> {
    bucket(PENDING_REWARD, storage)
}

pub fn pending_reward_read<S: ReadonlyStorage>(storage: &S) -> ReadonlyBucket<S, Uint128> {
    bucket_read(PENDING_REWARD, storage)
}
