use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub hub_contract: HumanAddr,
    pub bluna_reward_contract: HumanAddr,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    SwapToRewardDenom {
        bluna_total_bond_amount: Uint128,
        stluna_total_bond_amount: Uint128,
    },
    UpdateConfig {
        owner: Option<HumanAddr>,
        hub_contract: Option<HumanAddr>,
        bluna_reward_contract: Option<HumanAddr>,
        stluna_reward_denom: Option<String>,
        bluna_reward_denom: Option<String>,
    },
    DispatchRewards {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    // GetBufferedRewards returns the buffered amount of bLuna and stLuna rewards.
    GetBufferedRewards {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetBufferedRewardsResponse {}
