#![allow(clippy::field_reassign_with_default)] //https://github.com/CosmWasm/cosmwasm/issues/685

use crate::registry::Validator;
use cosmwasm_std::HumanAddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub registry: Vec<Validator>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    /// Adds a validator to the registry
    AddValidator { validator: Validator },

    /// Remove a validator from the registry
    RemoveValidator { address: HumanAddr },

    /// Update total_delegated field for validators in registry
    UpdateTotalDelegated { updated_validators: Vec<Validator> },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetValidatorsForDelegation returns validators sorted by available amount for delegation (delegation_limit - total_delegated)
    GetValidatorsForDelegation {},
}
