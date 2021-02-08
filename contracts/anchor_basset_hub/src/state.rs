use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, Decimal, HumanAddr, Order, ReadonlyStorage, StdError, StdResult, Storage,
    Uint128,
};
use cosmwasm_storage::{
    singleton, singleton_read, Bucket, PrefixedStorage, ReadonlyBucket, ReadonlyPrefixedStorage,
    ReadonlySingleton, Singleton,
};

use crate::msg::UnbondRequest;
use hub_querier::{Config, State};

pub type LastBatch = u64;

pub static CONFIG: &[u8] = b"config";
pub static STATE: &[u8] = b"state";
pub static PARAMETERS: &[u8] = b"parameteres";
pub static VALIDATORS: &[u8] = b"validators";

pub static PREFIX_WAIT_MAP: &[u8] = b"wait";
pub static CURRENT_BATCH: &[u8] = b"current_batch";
pub static UNBOND_HISTORY_MAP: &[u8] = b"history_map";
pub static PREFIX_AIRDROP_INFO: &[u8] = b"airedrop_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Parameters {
    pub epoch_period: u64,
    pub underlying_coin_denom: String,
    pub unbonding_period: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub reward_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatch {
    pub id: u64,
    pub requested_with_fee: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondHistory {
    pub batch_id: u64,
    pub time: u64,
    pub amount: Uint128,
    pub applied_exchange_rate: Decimal,
    pub withdraw_rate: Decimal,
    pub released: bool,
}

pub fn store_config<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, CONFIG)
}

pub fn read_config<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Config> {
    singleton_read(storage, CONFIG)
}

pub fn store_parameters<S: Storage>(storage: &mut S) -> Singleton<S, Parameters> {
    singleton(storage, PARAMETERS)
}

pub fn read_parameters<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Parameters> {
    singleton_read(storage, PARAMETERS)
}

pub fn store_current_batch<S: Storage>(storage: &mut S) -> Singleton<S, CurrentBatch> {
    singleton(storage, CURRENT_BATCH)
}

pub fn read_current_batch<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, CurrentBatch> {
    singleton_read(storage, CURRENT_BATCH)
}

pub fn store_state<S: Storage>(storage: &mut S) -> Singleton<S, State> {
    singleton(storage, STATE)
}

pub fn read_state<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, State> {
    singleton_read(storage, STATE)
}

/// Store undelegation wait list per each batch
/// HashMap<user's address, <batch_id, requested_amount>
pub fn store_unbond_wait_list<'a, S: Storage>(
    storage: &'a mut S,
    batch_id: u64,
    sender_address: HumanAddr,
    amount: Uint128,
) -> StdResult<()> {
    let batch = to_vec(&batch_id)?;
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<'a, S, Uint128> =
        Bucket::multilevel(&[PREFIX_WAIT_MAP, &addr], storage);
    position_indexer.update(&batch, |asked_already| {
        Ok(asked_already.unwrap_or_default() + amount)
    })?;

    Ok(())
}

/// Remove unbond batch id from user's wait list
pub fn remove_unbond_wait_list<'a, S: Storage>(
    storage: &'a mut S,
    batch_id: Vec<u64>,
    sender_address: HumanAddr,
) -> StdResult<()> {
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<'a, S, Uint128> =
        Bucket::multilevel(&[PREFIX_WAIT_MAP, &addr], storage);
    for b in batch_id {
        let batch = to_vec(&b)?;
        position_indexer.remove(&batch);
    }
    Ok(())
}

pub fn read_unbond_wait_list<'a, S: ReadonlyStorage>(
    storage: &'a S,
    batch_id: u64,
    sender_addr: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let batch = to_vec(&batch_id)?;
    res.load(&batch)
}

pub fn get_unbond_requests<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
) -> StdResult<UnbondRequest> {
    let vec = to_vec(&sender_addr)?;
    let mut requests: UnbondRequest = vec![];
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, value) = item.unwrap();
            let user_batch: u64 = from_slice(&k).unwrap();
            requests.push((user_batch, value))
        })
        .collect();
    Ok(requests)
}

pub fn get_unbond_batches<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
) -> StdResult<Vec<u64>> {
    let vec = to_vec(&sender_addr)?;
    let mut deprecated_batches: Vec<u64> = vec![];
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, _) = item.unwrap();
            let user_batch: u64 = from_slice(&k).unwrap();
            let history = read_unbond_history(storage, user_batch);
            if let Ok(h) = history {
                if h.released {
                    deprecated_batches.push(user_batch);
                }
            }
        })
        .collect();
    Ok(deprecated_batches)
}

/// Return all requested unbond amount.
/// This needs to be called after process withdraw rate function.
/// If the batch is released, this will return user's requested
/// amount proportional to withdraw rate.
pub fn get_finished_amount<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item.unwrap();
            let user_batch: u64 = from_slice(&k).unwrap();
            let history = read_unbond_history(storage, user_batch);
            if let Ok(h) = history {
                if h.released {
                    withdrawable_amount += v * h.withdraw_rate;
                }
            }
        })
        .collect();
    Ok(withdrawable_amount)
}

/// Return the finished amount for all batches that has been before the given block time.
pub fn query_get_finished_amount<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
    block_time: u64,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item.unwrap();
            let user_batch: u64 = from_slice(&k).unwrap();
            let history = read_unbond_history(storage, user_batch);
            if let Ok(h) = history {
                if h.time < block_time {
                    withdrawable_amount += v * h.withdraw_rate;
                }
            }
        })
        .collect();
    Ok(withdrawable_amount)
}

/// Store valid validators
pub fn store_white_validators<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    let value = to_vec(&true)?;
    PrefixedStorage::new(VALIDATORS, storage).set(&vec, &value);
    Ok(())
}

/// Remove valid validators
pub fn remove_white_validators<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    PrefixedStorage::new(VALIDATORS, storage).remove(&vec);
    Ok(())
}

// Returns all validators
pub fn read_validators<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    let res = ReadonlyPrefixedStorage::new(VALIDATORS, storage);
    let validators: Vec<HumanAddr> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let sender: HumanAddr = from_slice(&key).unwrap();
            sender
        })
        .collect();
    Ok(validators)
}

/// Check whether the validator is whitelisted.
pub fn is_valid_validator<S: Storage>(
    storage: &S,
    validator_address: HumanAddr,
) -> StdResult<bool> {
    let vec = to_vec(&validator_address)?;
    let res = ReadonlyPrefixedStorage::new(VALIDATORS, storage).get(&vec);
    match res {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

/// Read whitelisted validators
pub fn read_valid_validators<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    let res = ReadonlyPrefixedStorage::new(VALIDATORS, storage);
    let validators: Vec<HumanAddr> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let validator: HumanAddr = from_slice(&key).unwrap();
            validator
        })
        .collect();
    Ok(validators)
}

/// Store unbond history map
/// Hashmap<batch_id, <UnbondHistory>>
pub fn store_unbond_history<S: Storage>(
    storage: &mut S,
    batch_id: u64,
    history: UnbondHistory,
) -> StdResult<()> {
    let vec = batch_id.to_be_bytes().to_vec();
    let value: Vec<u8> = to_vec(&history)?;
    PrefixedStorage::new(UNBOND_HISTORY_MAP, storage).set(&vec, &value);
    Ok(())
}

#[allow(clippy::needless_lifetimes)]
pub fn read_unbond_history<'a, S: ReadonlyStorage>(
    storage: &'a S,
    epoc_id: u64,
) -> StdResult<UnbondHistory> {
    let vec = epoc_id.to_be_bytes().to_vec();
    let res = ReadonlyPrefixedStorage::new(UNBOND_HISTORY_MAP, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err(
            "Burn requests not found for the specified time period",
        )),
    }
}

// settings for pagination
const MAX_LIMIT: u32 = 100;
const DEFAULT_LIMIT: u32 = 10;

/// Return all unbond_history from UnbondHistory map
#[allow(clippy::needless_lifetimes)]
pub fn all_unbond_history<'a, S: ReadonlyStorage>(
    storage: &'a S,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<Vec<UnbondHistory>> {
    let vec = convert(start);

    let lim = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let res = ReadonlyPrefixedStorage::new(UNBOND_HISTORY_MAP, storage)
        .range(vec.as_deref(), None, Order::Ascending)
        .take(lim)
        .map(|item| {
            let history: UnbondHistory = from_slice(&item.1).unwrap();
            Ok(history)
        })
        .collect();
    res
}

fn convert(start_after: Option<u64>) -> Option<Vec<u8>> {
    start_after.map(|idx| {
        let mut v = idx.to_be_bytes().to_vec();
        v.push(1);
        v
    })
}
