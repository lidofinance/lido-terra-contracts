use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{Decimal, HumanAddr, ReadonlyStorage, Storage, Uint128};
use cosmwasm_storage::{
    bucket, bucket_read, singleton, singleton_read, Bucket, ReadonlyBucket, ReadonlySingleton,
    Singleton,
};
use std::collections::HashMap;

pub static TOKEN_STATE_KEY: &[u8] = b"token_state";
pub static TOKEN_INFO_KEY: &[u8] = b"token_info";
pub static POOL_INFO: &[u8] = b"pool_info";
const BALANCE: &[u8] = b"balance";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenState {
    pub delegation_map: HashMap<HumanAddr, Uint128>,
    pub holder_map: HashMap<HumanAddr, Decimal>,
    pub undelegated_wait_list_map: HashMap<HumanAddr, Uint128>,
    pub redeem_wait_list_map: HashMap<HumanAddr, Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolInfo {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub total_issued: Uint128,
    pub claimed: Uint128,
    pub reward_index: Decimal,
}

impl Default for PoolInfo {
    fn default() -> Self {
        Self {
            exchange_rate: Decimal::one(),
            total_bond_amount: Default::default(),
            total_issued: Default::default(),
            claimed: Default::default(),
            reward_index: Default::default(),
        }
    }
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

pub fn token_state<S: Storage>(storage: &mut S) -> Singleton<S, TokenState> {
    singleton(storage, TOKEN_STATE_KEY)
}

pub fn token_state_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, TokenState> {
    singleton_read(storage, TOKEN_STATE_KEY)
}

pub fn pool_info<S: Storage>(storage: &mut S) -> Singleton<S, PoolInfo> {
    singleton(storage, POOL_INFO)
}

pub fn pool_info_read<S: ReadonlyStorage>(storage: &S) -> ReadonlySingleton<S, PoolInfo> {
    singleton_read(storage, POOL_INFO)
}
