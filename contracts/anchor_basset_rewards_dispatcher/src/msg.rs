use cosmwasm_std::{Decimal, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub hub_contract: String,
    pub bluna_reward_contract: String,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
    pub lido_fee_address: String,
    pub lido_fee_rate: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    SwapToRewardDenom {
        bluna_total_mint_amount: Uint128,
        stluna_total_mint_amount: Uint128,
    },
    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
        bluna_reward_contract: Option<String>,
        stluna_reward_denom: Option<String>,
        bluna_reward_denom: Option<String>,
        lido_fee_address: Option<String>,
        lido_fee_rate: Option<Decimal>,
    },
    DispatchRewards {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetBufferedRewards returns the buffered amount of bLuna and stLuna rewards.
    GetBufferedRewards {},
    // Config returns config
    Config {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetBufferedRewardsResponse {}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
