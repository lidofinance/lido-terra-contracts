use crate::hook::InitHook;
use cosmwasm_std::CanonicalAddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RewardInitMsg {
    pub owner: CanonicalAddr,
    pub init_hook: Option<InitHook>,
}
