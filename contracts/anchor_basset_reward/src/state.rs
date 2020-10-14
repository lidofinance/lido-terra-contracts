use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::hook::InitHook;
use cosmwasm_std::CanonicalAddr;

pub static CONFIG_KEY: &[u8] = b"config";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: CanonicalAddr,
    pub init_hook: Option<InitHook>,
}
