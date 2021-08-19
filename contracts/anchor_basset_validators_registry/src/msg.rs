use crate::registry::Validator;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub registry: Vec<Validator>,
    pub hub_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Adds a validator to the registry
    AddValidator { validator: Validator },

    /// Remove a validator from the registry
    RemoveValidator { address: String },

    /// Update config
    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetValidatorsForDelegation returns validators sorted by available amount for delegation (delegation_limit - total_delegated)
    GetValidatorsForDelegation {},
}
