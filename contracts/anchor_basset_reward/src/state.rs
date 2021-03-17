use cosmwasm_std::{
    Api, CanonicalAddr, Decimal, Extern, HumanAddr, Order, Querier, ReadonlyStorage, StdResult,
    Storage, Uint128,
};
use cosmwasm_storage::{bucket, bucket_read, singleton, singleton_read, ReadonlyBucket};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use reward_querier::HolderResponse;

pub static KEY_CONFIG: &[u8] = b"config";
pub static KEY_STATE: &[u8] = b"state";

pub static PREFIX_HOLDERS: &[u8] = b"holders";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub hub_contract: CanonicalAddr,
    pub reward_denom: String,
}

pub fn store_config<S: Storage>(storage: &mut S, config: &Config) -> StdResult<()> {
    singleton(storage, KEY_CONFIG).save(config)
}

pub fn read_config<S: ReadonlyStorage>(storage: &S) -> StdResult<Config> {
    singleton_read(storage, KEY_CONFIG).load()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct State {
    pub global_index: Decimal,
    pub total_balance: Uint128,
    pub prev_reward_balance: Uint128,
}

pub fn store_state<S: Storage>(storage: &mut S, state: &State) -> StdResult<()> {
    singleton(storage, KEY_STATE).save(state)
}

pub fn read_state<S: ReadonlyStorage>(storage: &S) -> StdResult<State> {
    singleton_read(storage, KEY_STATE).load()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Holder {
    pub balance: Uint128,
    pub index: Decimal,
    pub pending_rewards: Decimal,
}

// This is similar to HashMap<holder's address, Hodler>
pub fn store_holder<S: Storage>(
    storage: &mut S,
    holder_address: &CanonicalAddr,
    holder: &Holder,
) -> StdResult<()> {
    bucket(PREFIX_HOLDERS, storage).save(holder_address.as_slice(), holder)
}

pub fn read_holder<S: Storage>(storage: &S, holder_address: &CanonicalAddr) -> StdResult<Holder> {
    let res: Option<Holder> =
        bucket_read(PREFIX_HOLDERS, storage).may_load(holder_address.as_slice())?;
    match res {
        Some(holder) => Ok(holder),
        None => Ok(Holder {
            balance: Uint128::zero(),
            index: Decimal::zero(),
            pending_rewards: Decimal::zero(),
        }),
    }
}

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_holders<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    start_after: Option<CanonicalAddr>,
    limit: Option<u32>,
) -> StdResult<Vec<HolderResponse>> {
    let holder_bucket: ReadonlyBucket<S, Holder> = bucket_read(PREFIX_HOLDERS, &deps.storage);

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after);

    holder_bucket
        .range(start.as_deref(), None, Order::Ascending)
        .take(limit)
        .map(|elem| {
            let (k, v) = elem?;
            let address: HumanAddr = deps.api.human_address(&CanonicalAddr::from(k))?;
            Ok(HolderResponse {
                address,
                balance: v.balance,
                index: v.index,
                pending_rewards: v.pending_rewards,
            })
        })
        .collect()
}

// this will set the first key after the provided key, by appending a 1 byte
fn calc_range_start(start_after: Option<CanonicalAddr>) -> Option<Vec<u8>> {
    start_after.map(|addr| {
        let mut v = addr.as_slice().to_vec();
        v.push(1);
        v
    })
}
