use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_vec, CanonicalAddr, HumanAddr, Order, StdResult, Storage};
use cosmwasm_storage::{Bucket, ReadonlyBucket, ReadonlySingleton, Singleton};

pub static KEY_CONFIG: &[u8] = b"config";
pub static PREFIX_AIRODROP_INFO: &[u8] = b"airdrop_info";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub hub_contract: HumanAddr,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfo {
    pub airdrop_token_contract: HumanAddr,
    pub airdrop_contract: HumanAddr,
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

pub fn read_airdrop_info<S: Storage>(storage: &S, airdrop_token: String) -> AirdropInfo {
    let key = to_vec(&airdrop_token).unwrap();
    let airdrop_bucket: ReadonlyBucket<S, AirdropInfo> =
        ReadonlyBucket::new(PREFIX_AIRODROP_INFO, storage);
    match airdrop_bucket.load(&key) {
        Ok(v) => v,
        _ => AirdropInfo {
            airdrop_contract: HumanAddr::default(),
            airdrop_token_contract: HumanAddr::default(),
        },
    }
}

pub fn read_all_airdrop_infos<S: Storage>(storage: &S) -> StdResult<Vec<AirdropInfo>> {
    let airdrop_bucket: ReadonlyBucket<S, AirdropInfo> =
        ReadonlyBucket::new(PREFIX_AIRODROP_INFO, storage);
    let infos: Vec<AirdropInfo> = airdrop_bucket
        .range(None, None, Order::Ascending)
        .map(|item| {
            let (_k, v) = item.unwrap();
            v
        })
        .collect();
    Ok(infos)
}
