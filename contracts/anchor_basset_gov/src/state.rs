use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, HumanAddr, Order, ReadonlyStorage, StdError, StdResult,
    Storage, Uint128,
};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, PrefixedStorage, ReadonlyBucket,
    ReadonlyPrefixedStorage, ReadonlySingleton, Singleton,
};
use std::collections::HashMap;

use gov_courier::PoolInfo;

// EPOC = 21600s is equal to 6 hours
pub const EPOC: u64 = 21600;

pub static CONFIG: &[u8] = b"gov_config";
pub static POOL_INFO: &[u8] = b"pool_info";
static PREFIX_REWARD: &[u8] = b"claim";

pub static PREFIX_UNBOUND_PER_EPOC: &[u8] = b"unbound";
pub static PREFIX_DELEGATION_MAP: &[u8] = b"delegate";
pub static PREFIX_WAIT_MAP: &[u8] = b"wait";
pub static EPOC_ID: &[u8] = b"epoc";
pub static VALIDATORS: &[u8] = b"validators";
pub static ALL_EPOC_ID: &[u8] = b"epoc_list";

pub static WHITE_VALIDATORS: &[u8] = b"white_validators";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GovConfig {
    pub creator: CanonicalAddr,
}

#[derive(
    PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Clone, JsonSchema, Debug, Copy,
)]
pub struct EpocId {
    pub epoc_id: u64,
    pub current_block_time: u64,
}

impl EpocId {
    pub fn compute_current_epoc(&mut self, block_time: u64) {
        let epoc = self.epoc_id;
        let time = self.current_block_time;

        self.current_block_time = block_time;
        self.epoc_id = epoc + (block_time - time) / EPOC;
    }

    pub fn is_epoc_passed(&self, block_time: u64) -> bool {
        let time = self.current_block_time;
        if (block_time - time) < EPOC {
            return false;
        }
        true
    }
}

//keep all unprocessed epocs
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct AllEpoc {
    pub epoces: Vec<EpocId>,
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, GovConfig> {
    singleton(storage, CONFIG)
}

pub fn config_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, GovConfig> {
    singleton_read(storage, CONFIG)
}

pub fn save_epoc<S: Storage>(storage: &mut S) -> Singleton<S, EpocId> {
    singleton(storage, EPOC_ID)
}

pub fn epoc_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, EpocId> {
    singleton_read(storage, EPOC_ID)
}

pub fn save_all_epoc<S: Storage>(storage: &mut S) -> Singleton<S, AllEpoc> {
    singleton(storage, ALL_EPOC_ID)
}

pub fn read_all_epocs<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, AllEpoc> {
    singleton_read(storage, ALL_EPOC_ID)
}

pub fn pool_info<S: Storage>(storage: &mut S) -> Singleton<S, PoolInfo> {
    singleton(storage, POOL_INFO)
}

pub fn pool_info_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, PoolInfo> {
    singleton_read(storage, POOL_INFO)
}

pub fn claim_store<S: Storage>(storage: &mut S) -> Bucket<S, Uint128> {
    bucket(PREFIX_REWARD, storage)
}

pub fn claim_read<S: ReadonlyStorage>(storage: &S) -> ReadonlyBucket<S, Uint128> {
    bucket_read(PREFIX_REWARD, storage)
}

//this stores unboned amount in the storage per each epoc.
pub fn store_total_amount<S: Storage>(
    storage: &mut S,
    epoc_id: u64,
    claimed: Uint128,
) -> StdResult<()> {
    let vec = to_vec(&epoc_id)?;
    let value: Vec<u8> = to_vec(&claimed)?;
    PrefixedStorage::new(PREFIX_UNBOUND_PER_EPOC, storage).set(&vec, &value);
    Ok(())
}

pub fn read_total_amount<S: Storage>(storage: &S, epoc_id: u64) -> StdResult<Uint128> {
    let vec = to_vec(&epoc_id)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_UNBOUND_PER_EPOC, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err("no unbond amount is found")),
    }
}

// maps address of validator address to amount that the contract has delegated to
pub fn store_delegation_map<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
    amount: Uint128,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    let value = to_vec(&amount)?;
    PrefixedStorage::new(PREFIX_DELEGATION_MAP, storage).set(&vec, &value);
    Ok(())
}

pub fn read_delegation_map<S: Storage>(
    storage: &S,
    validator_address: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&validator_address)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_DELEGATION_MAP, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err("no validator is found")),
    }
}

// Returns all validators and their delegated amount
pub fn read_validators<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    let mut validators: Vec<HumanAddr> = Vec::new();
    let res = ReadonlyPrefixedStorage::new(PREFIX_DELEGATION_MAP, storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let sender: HumanAddr = from_slice(&key).unwrap();
            validators.push(sender);
        })
        .collect();

    Ok(validators)
}

//stores undelegation wait list per each epoc.
pub fn store_undelegated_wait_list<'a, S: Storage>(
    storage: &'a mut S,
    epoc_id: u64,
    sender_address: HumanAddr,
    amount: Uint128,
) -> StdResult<()> {
    let vec = to_vec(&epoc_id)?;
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<'a, S, Uint128> =
        Bucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    position_indexer.save(&addr, &amount)?;

    Ok(())
}

pub fn read_undelegated_wait_list<'a, S: ReadonlyStorage>(
    storage: &'a S,
    epoc_id: u64,
    sender_addr: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&epoc_id)?;
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let amount = res.load(sender_addr.0.as_bytes());
    amount
}

pub fn read_undelegated_wait_list_for_epoc<'a, S: ReadonlyStorage>(
    storage: &'a S,
    epoc_id: u64,
) -> StdResult<HashMap<HumanAddr, Uint128>> {
    let vec = to_vec(&epoc_id)?;
    let mut list: HashMap<HumanAddr, Uint128> = HashMap::new();
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);

    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item.unwrap();
            let key: HumanAddr = from_slice(&k).unwrap();
            list.insert(key, v)
        })
        .collect();
    println!("get the undelegate {}, epoc_id {}", list.len(), epoc_id);
    Ok(list)
}

// store valid validators
pub fn store_white_validators<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    let value = to_vec(&true)?;
    PrefixedStorage::new(PREFIX_DELEGATION_MAP, storage).set(&vec, &value);
    Ok(())
}

// remove valid validators
pub fn remove_white_validators<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    PrefixedStorage::new(PREFIX_DELEGATION_MAP, storage).remove(&vec);
    Ok(())
}

pub fn is_valid_validator<S: Storage>(
    storage: &S,
    validator_address: HumanAddr,
) -> StdResult<bool> {
    let vec = to_vec(&validator_address)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_DELEGATION_MAP, storage).get(&vec);
    match res {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

pub fn read_valid_validators<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    let mut validators: Vec<HumanAddr> = Vec::new();
    let res = ReadonlyPrefixedStorage::new(PREFIX_DELEGATION_MAP, storage);

    let _: Vec<()> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let validator: HumanAddr = from_slice(&key).unwrap();
            validators.push(validator);
        })
        .collect();
    Ok(validators)
}
