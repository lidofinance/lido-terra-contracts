use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, Addr, Decimal, Order, StdError, StdResult, Storage, Uint128,
};
use cosmwasm_storage::{Bucket, PrefixedStorage, ReadonlyBucket, ReadonlyPrefixedStorage};
use cw_storage_plus::Item;

use basset::hub::{Config, State, UnbondHistory, UnbondRequest};

pub type LastBatch = u64;

pub static PREFIX_WAIT_MAP: &[u8] = b"wait";
pub static PREFIX_AIRDROP_INFO: &[u8] = b"airedrop_info";
pub static UNBOND_HISTORY_MAP: &[u8] = b"history_map";
pub static VALIDATORS: &[u8] = b"validators";

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

pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");
pub const PARAMETERS: Item<Parameters> = Item::new("\u{0}\u{6}parameteres");
pub const CURRENT_BATCH: Item<CurrentBatch> = Item::new("\u{0}\u{6}current_batch");
pub const STATE: Item<State> = Item::new("\u{0}\u{6}state");

/// Store undelegation wait list per each batch
/// HashMap<user's address, <batch_id, requested_amount>
pub fn store_unbond_wait_list(
    storage: &mut dyn Storage,
    batch_id: u64,
    sender_address: String,
    amount: Uint128,
) -> StdResult<()> {
    let batch = to_vec(&batch_id)?;
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<Uint128> =
        Bucket::multilevel(storage, &[PREFIX_WAIT_MAP, &addr]);
    position_indexer.update(&batch, |asked_already| -> StdResult<Uint128> {
        Ok(asked_already.unwrap_or_default() + amount)
    })?;

    Ok(())
}

/// Remove unbond batch id from user's wait list
pub fn remove_unbond_wait_list(
    storage: &mut dyn Storage,
    batch_id: Vec<u64>,
    sender_address: Addr,
) -> StdResult<()> {
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<Uint128> =
        Bucket::multilevel(storage, &[PREFIX_WAIT_MAP, &addr]);
    for b in batch_id {
        let batch = to_vec(&b)?;
        position_indexer.remove(&batch);
    }
    Ok(())
}

pub fn read_unbond_wait_list(
    storage: &dyn Storage,
    batch_id: u64,
    sender_addr: String,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let res: ReadonlyBucket<Uint128> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_WAIT_MAP, &vec]);
    let batch = to_vec(&batch_id)?;
    res.load(&batch)
}

pub fn get_unbond_requests(storage: &dyn Storage, sender_addr: String) -> StdResult<UnbondRequest> {
    let vec = to_vec(&sender_addr)?;
    let mut requests: UnbondRequest = vec![];

    let res: ReadonlyBucket<Uint128> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_WAIT_MAP, &vec]);
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

pub fn get_unbond_batches(storage: &dyn Storage, sender_addr: String) -> StdResult<Vec<u64>> {
    let vec = to_vec(&sender_addr)?;
    let mut deprecated_batches: Vec<u64> = vec![];
    let res: ReadonlyBucket<Uint128> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_WAIT_MAP, &vec]);
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
pub fn get_finished_amount(storage: &dyn Storage, sender_addr: String) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let res: ReadonlyBucket<Uint128> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_WAIT_MAP, &vec]);
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
pub fn query_get_finished_amount(
    storage: &dyn Storage,
    sender_addr: String,
    block_time: u64,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let mut withdrawable_amount: Uint128 = Uint128::zero();
    let res: ReadonlyBucket<Uint128> =
        ReadonlyBucket::multilevel(storage, &[PREFIX_WAIT_MAP, &vec]);
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
pub fn store_white_validators(
    storage: &mut dyn Storage,
    validator_address: String,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    let value = to_vec(&true)?;
    PrefixedStorage::new(storage, VALIDATORS).set(&vec, &value);
    Ok(())
}

/// Remove valid validators
pub fn remove_white_validators(
    storage: &mut dyn Storage,
    validator_address: String,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    PrefixedStorage::new(storage, VALIDATORS).remove(&vec);
    Ok(())
}

// Returns all validators
pub fn read_validators(storage: &dyn Storage) -> StdResult<Vec<String>> {
    let res = ReadonlyPrefixedStorage::new(storage, VALIDATORS);
    let validators: Vec<String> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let sender: String = from_slice(&key).unwrap();
            sender
        })
        .collect();
    Ok(validators)
}

/// Check whether the validator is whitelisted.
pub fn is_valid_validator(storage: &dyn Storage, validator_address: String) -> StdResult<bool> {
    let vec = to_vec(&validator_address)?;
    let res = ReadonlyPrefixedStorage::new(storage, VALIDATORS).get(&vec);
    match res {
        Some(_) => Ok(true),
        None => Ok(false),
    }
}

/// Read whitelisted validators
pub fn read_valid_validators(storage: &dyn Storage) -> StdResult<Vec<String>> {
    let res = ReadonlyPrefixedStorage::new(storage, VALIDATORS);
    let validators: Vec<String> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (key, _) = item;
            let sender: String = from_slice(&key).unwrap();
            sender
        })
        .collect();
    Ok(validators)
}

/// Store unbond history map
/// Hashmap<batch_id, <UnbondHistory>>
pub fn store_unbond_history(
    storage: &mut dyn Storage,
    batch_id: u64,
    history: UnbondHistory,
) -> StdResult<()> {
    let vec = batch_id.to_be_bytes().to_vec();
    let value: Vec<u8> = to_vec(&history)?;
    PrefixedStorage::new(storage, UNBOND_HISTORY_MAP).set(&vec, &value);
    Ok(())
}

pub fn read_unbond_history(storage: &dyn Storage, epoc_id: u64) -> StdResult<UnbondHistory> {
    let vec = epoc_id.to_be_bytes().to_vec();
    let res = ReadonlyPrefixedStorage::new(storage, UNBOND_HISTORY_MAP).get(&vec);
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
pub fn all_unbond_history(
    storage: &dyn Storage,
    start: Option<u64>,
    limit: Option<u32>,
) -> StdResult<Vec<UnbondHistory>> {
    let vec = convert(start);

    let lim = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let res = ReadonlyPrefixedStorage::new(storage, UNBOND_HISTORY_MAP)
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
