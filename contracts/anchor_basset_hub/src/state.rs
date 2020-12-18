use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, Decimal, HumanAddr, Order, ReadonlyStorage, StdError,
    StdResult, Storage, Uint128,
};
use cosmwasm_storage::{
    singleton, singleton_read, Bucket, PrefixedStorage, ReadonlyBucket, ReadonlyPrefixedStorage,
    ReadonlySingleton, Singleton,
};

use crate::msg::UnbondRequest;
use hub_courier::{Deactivated, PoolInfo};

pub static CONFIG: &[u8] = b"hub_config";
pub static POOL_INFO: &[u8] = b"pool_info";
pub static PARAMETERS: &[u8] = b"parameteres";
pub static MSG_STATUS: &[u8] = b"msg_status";

pub static PREFIX_UNBONDED_PER_EPOCH: &[u8] = b"unbond";
pub static VALIDATORS: &[u8] = b"validators";
pub static PREFIX_WAIT_MAP: &[u8] = b"wait";
pub static EPOCH_ID: &[u8] = b"epoch";

pub static SLASHING: &[u8] = b"slashing";
pub static BONDED: &[u8] = b"bonded";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub creator: CanonicalAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Parameters {
    pub epoch_time: u64,
    pub underlying_coin_denom: String,
    pub undelegated_epoch: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub reward_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MsgStatus {
    pub slashing: Option<Deactivated>,
    pub burn: Option<Deactivated>,
}

#[derive(
    PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Clone, JsonSchema, Debug, Copy,
)]
pub struct EpochId {
    pub epoch_id: u64,
    pub current_block_time: u64,
}

impl EpochId {
    pub fn compute_current_epoch(&mut self, block_time: u64, epoch_time: u64) {
        let epoc = self.epoch_id;
        let time = self.current_block_time;

        self.current_block_time = block_time;
        self.epoch_id = epoc + (block_time - time) / epoch_time;
    }

    pub fn is_epoch_passed(&self, block_time: u64, epoch_time: u64) -> bool {
        let time = self.current_block_time;
        if (block_time - time) < epoch_time {
            return false;
        }
        true
    }
}

pub fn config<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    singleton(storage, CONFIG)
}

pub fn config_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Config> {
    singleton_read(storage, CONFIG)
}

pub fn parameters<S: Storage>(storage: &mut S) -> Singleton<S, Parameters> {
    singleton(storage, PARAMETERS)
}

pub fn parameters_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Parameters> {
    singleton_read(storage, PARAMETERS)
}

pub fn msg_status<S: Storage>(storage: &mut S) -> Singleton<S, MsgStatus> {
    singleton(storage, MSG_STATUS)
}

pub fn msg_status_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, MsgStatus> {
    singleton_read(storage, MSG_STATUS)
}

pub fn save_epoch<S: Storage>(storage: &mut S) -> Singleton<S, EpochId> {
    singleton(storage, EPOCH_ID)
}

pub fn epoch_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, EpochId> {
    singleton_read(storage, EPOCH_ID)
}

pub fn pool_info<S: Storage>(storage: &mut S) -> Singleton<S, PoolInfo> {
    singleton(storage, POOL_INFO)
}

pub fn pool_info_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, PoolInfo> {
    singleton_read(storage, POOL_INFO)
}

//this stores unboned amount in the storage per each epoc.
pub fn store_total_amount<S: Storage>(
    storage: &mut S,
    epoc_id: u64,
    claimed: Uint128,
) -> StdResult<()> {
    let vec = to_vec(&epoc_id)?;
    let value: Vec<u8> = to_vec(&claimed)?;
    PrefixedStorage::new(PREFIX_UNBONDED_PER_EPOCH, storage).set(&vec, &value);
    Ok(())
}

pub fn read_total_amount<S: Storage>(storage: &S, epoc_id: u64) -> StdResult<Uint128> {
    let vec = to_vec(&epoc_id)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_UNBONDED_PER_EPOCH, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err("no unbond amount is found")),
    }
}

//store undelegation wait list per each epoc
pub fn store_undelegated_wait_list<'a, S: Storage>(
    storage: &'a mut S,
    epoc_id: u64,
    sender_address: HumanAddr,
    amount: Uint128,
) -> StdResult<()> {
    let epoch = to_vec(&epoc_id)?;
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<'a, S, Uint128> =
        Bucket::multilevel(&[PREFIX_WAIT_MAP, &addr], storage);
    position_indexer.update(&epoch, |asked_already| {
        Ok(asked_already.unwrap_or_default() + amount)
    })?;

    Ok(())
}

//store undelegation wait list per each epoc
pub fn remove_undelegated_wait_list<'a, S: Storage>(
    storage: &'a mut S,
    epoc_id: Vec<u64>,
    sender_address: HumanAddr,
) -> StdResult<()> {
    let addr = to_vec(&sender_address)?;
    let mut position_indexer: Bucket<'a, S, Uint128> =
        Bucket::multilevel(&[PREFIX_WAIT_MAP, &addr], storage);
    for e in epoc_id {
        let epoch = to_vec(&e)?;
        position_indexer.remove(&epoch);
    }
    Ok(())
}

pub fn read_undelegated_wait_list<'a, S: ReadonlyStorage>(
    storage: &'a S,
    epoc_id: u64,
    sender_addr: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let epoch = to_vec(&epoc_id)?;
    res.load(&epoch)
}

//this function is here for test purpose
pub fn get_burn_requests_epochs<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
) -> StdResult<Vec<u64>> {
    let vec = to_vec(&sender_addr)?;
    let mut amount: Vec<u64> = vec![];
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, _) = item.unwrap();
            let epoch: u64 = from_slice(&k).unwrap();
            amount.push(epoch)
        })
        .collect();
    Ok(amount)
}

pub fn get_burn_requests<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
) -> StdResult<UnbondRequest> {
    let vec = to_vec(&sender_addr)?;
    let mut request: UnbondRequest = vec![];
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, value) = item.unwrap();
            let user_epoch: u64 = from_slice(&k).unwrap();
            request.push((user_epoch, value));
        })
        .collect();
    Ok(request)
}

pub fn get_burn_epochs<'a, S: ReadonlyStorage>(
    storage: &'a S,
    sender_addr: HumanAddr,
    epoc_id: u64,
) -> StdResult<Vec<u64>> {
    let vec = to_vec(&sender_addr)?;
    let mut deprecated_epochs: Vec<u64> = vec![];
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, _) = item.unwrap();
            let user_epoch: u64 = from_slice(&k).unwrap();
            if user_epoch < epoc_id {
                deprecated_epochs.push(user_epoch);
            }
        })
        .collect();
    Ok(deprecated_epochs)
}

//return all requested burn amount that has been requested from 24 days ago.
pub fn get_finished_amount<'a, S: ReadonlyStorage>(
    storage: &'a S,
    epoc_id: u64,
    sender_addr: HumanAddr,
) -> StdResult<Uint128> {
    let vec = to_vec(&sender_addr)?;
    let mut amount: Uint128 = Uint128::zero();
    let res: ReadonlyBucket<'a, S, Uint128> =
        ReadonlyBucket::multilevel(&[PREFIX_WAIT_MAP, &vec], storage);
    let _un: Vec<_> = res
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (k, v) = item.unwrap();
            let user_epoch: u64 = from_slice(&k).unwrap();
            if user_epoch < epoc_id {
                amount += v;
            }
        })
        .collect();
    Ok(amount)
}

// store valid validators
pub fn store_white_validators<S: Storage>(
    storage: &mut S,
    validator_address: HumanAddr,
) -> StdResult<()> {
    let vec = to_vec(&validator_address)?;
    let value = to_vec(&true)?;
    PrefixedStorage::new(VALIDATORS, storage).set(&vec, &value);
    Ok(())
}

// remove valid validators
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
    let mut validators: Vec<HumanAddr> = Vec::new();
    let res = ReadonlyPrefixedStorage::new(VALIDATORS, storage);
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

pub fn read_valid_validators<S: Storage>(storage: &S) -> StdResult<Vec<HumanAddr>> {
    let mut validators: Vec<HumanAddr> = Vec::new();
    let res = ReadonlyPrefixedStorage::new(VALIDATORS, storage);

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
pub fn set_all_delegations<S: Storage>(storage: &mut S) -> Singleton<S, Uint128> {
    singleton(storage, SLASHING)
}

pub fn get_all_delegations<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Uint128> {
    singleton_read(storage, SLASHING)
}

pub fn set_bonded<S: Storage>(storage: &mut S) -> Singleton<S, Uint128> {
    singleton(storage, BONDED)
}

pub fn get_bonded<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, Uint128> {
    singleton_read(storage, BONDED)
}
