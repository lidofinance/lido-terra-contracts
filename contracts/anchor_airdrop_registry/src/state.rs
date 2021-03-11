use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::msg::AirdropInfoElem;
use cosmwasm_std::{
    from_slice, to_vec, CanonicalAddr, Decimal, HumanAddr, Order, StdResult, Storage,
};
use cosmwasm_storage::{Bucket, ReadonlyBucket, ReadonlySingleton, Singleton};

pub static KEY_CONFIG: &[u8] = b"config";
pub static PREFIX_AIRODROP_INFO: &[u8] = b"airdrop_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: HumanAddr,
    pub reward_contract: HumanAddr,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfo {
    pub airdrop_token_contract: HumanAddr,
    pub airdrop_contract: HumanAddr,
    pub airdrop_swap_contract: HumanAddr,
    pub swap_belief_price: Option<Decimal>,
    pub swap_max_spread: Option<Decimal>,
}

pub fn store_config<S: Storage>(storage: &mut S) -> Singleton<S, Config> {
    Singleton::new(storage, KEY_CONFIG)
}

pub fn read_config<S: Storage>(storage: &S) -> ReadonlySingleton<S, Config> {
    ReadonlySingleton::new(storage, KEY_CONFIG)
}

pub fn store_airdrop_info<S: Storage>(
    storage: &mut S,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    let mut airdrop_bucket: Bucket<S, AirdropInfo> = Bucket::new(PREFIX_AIRODROP_INFO, storage);
    airdrop_bucket.save(&key, &airdrop_info)?;

    Ok(())
}

pub fn update_airdrop_info<S: Storage>(
    storage: &mut S,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    let mut airdrop_bucket: Bucket<S, AirdropInfo> = Bucket::new(PREFIX_AIRODROP_INFO, storage);
    airdrop_bucket.update(&key, |_| Ok(airdrop_info))?;

    Ok(())
}

pub fn remove_airdrop_info<S: Storage>(storage: &mut S, airdrop_token: String) -> StdResult<()> {
    let key = to_vec(&airdrop_token)?;
    let mut airdrop_bucket: Bucket<S, AirdropInfo> = Bucket::new(PREFIX_AIRODROP_INFO, storage);
    airdrop_bucket.remove(&key);

    Ok(())
}

pub fn read_airdrop_info<S: Storage>(storage: &S, airdrop_token: String) -> StdResult<AirdropInfo> {
    let key = to_vec(&airdrop_token).unwrap();
    let airdrop_bucket: ReadonlyBucket<S, AirdropInfo> =
        ReadonlyBucket::new(PREFIX_AIRODROP_INFO, storage);
    airdrop_bucket.load(&key)
}

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;
pub fn read_all_airdrop_infos<S: Storage>(
    storage: &S,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<AirdropInfoElem>> {
    let airdrop_bucket: ReadonlyBucket<S, AirdropInfo> =
        ReadonlyBucket::new(PREFIX_AIRODROP_INFO, storage);

    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = calc_range_start(start_after);

    let infos: Vec<AirdropInfoElem> = airdrop_bucket
        .range(start.as_deref(), None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (k, v) = item.unwrap();
            let key: String = from_slice(&k).unwrap();
            AirdropInfoElem {
                airdrop_token: key,
                info: v,
            }
        })
        .collect();

    Ok(infos)
}

fn calc_range_start(start_after: Option<String>) -> Option<Vec<u8>> {
    start_after.map(|air| {
        let mut v = to_vec(&air).unwrap();
        v.push(1);
        v
    })
}
