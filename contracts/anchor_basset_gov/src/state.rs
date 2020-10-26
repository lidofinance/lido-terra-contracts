use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, Decimal, HumanAddr, Order, ReadonlyStorage, StdError,
    StdResult, Storage, Uint128,
};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, PrefixedStorage, ReadonlyBucket,
    ReadonlyPrefixedStorage, ReadonlySingleton, Singleton,
};
use std::collections::HashMap;

// EPOC = 21600s is equal to 6 hours
pub const EPOC: u64 = 21600;

pub static TOKEN_INFO_KEY: &[u8] = b"token_info";
pub static POOL_INFO: &[u8] = b"pool_info";
const BALANCE: &[u8] = b"balance";
static PREFIX_REWARD: &[u8] = b"claim";

pub static PREFIX_UNBOUND_PER_EPOC: &[u8] = b"unbound";
pub static PREFIX_DELEGATION_MAP: &[u8] = b"delegate";
pub static PREFIX_HOLDER_MAP: &[u8] = b"holder";
pub static PREFIX_WAIT_MAP: &[u8] = b"wait";
pub static EPOC_ID: &[u8] = b"epoc";
pub static VALIDATORS: &[u8] = b"validators";
pub static ALL_EPOC_ID: &[u8] = b"epoc_list";

pub static WHITE_VALIDATORS: &[u8] = b"white_validators";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
    pub creator: CanonicalAddr,
    //TODO: Add Undelegation Period as a TokenInfo which should be changed.
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct PoolInfo {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub total_issued: Uint128,
    pub claimed: Uint128,
    pub reward_index: Decimal,
    pub current_block_time: u64,
    pub all_reward: Uint128,
    pub reward_account: CanonicalAddr,
    // This helps to control Register message.
    // Register message should be called once
    pub is_reward_exist: bool,
}

impl PoolInfo {
    pub fn update_exchange_rate(&mut self) {
        //FIXME: Is total supply equal to total issued?
        self.exchange_rate = Decimal::from_ratio(self.total_bond_amount, self.total_issued);
    }
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

// This is similar to HashMap<holder's address, reward_index>
pub fn store_holder_map<S: Storage>(
    storage: &mut S,
    holder_address: HumanAddr,
    index: Decimal,
) -> StdResult<()> {
    let vec = to_vec(&holder_address)?;
    let value: Vec<u8> = to_vec(&index)?;
    PrefixedStorage::new(PREFIX_HOLDER_MAP, storage).set(&vec, &value);
    Ok(())
}

pub fn read_holder_map<S: Storage>(storage: &S, holder_address: HumanAddr) -> StdResult<Decimal> {
    let vec = to_vec(&holder_address)?;
    let res = ReadonlyPrefixedStorage::new(PREFIX_HOLDER_MAP, storage).get(&vec);
    match res {
        Some(data) => from_slice(&data),
        None => Err(StdError::generic_err("no holder is found")),
    }
}

// Returns a HashMap of holders. <holders, reward_index>
pub fn read_holders<S: Storage>(storage: &S) -> StdResult<HashMap<HumanAddr, Decimal>> {
    let mut holders: HashMap<HumanAddr, Decimal> = HashMap::new();
    let res = ReadonlyPrefixedStorage::new(PREFIX_HOLDER_MAP, storage);
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
