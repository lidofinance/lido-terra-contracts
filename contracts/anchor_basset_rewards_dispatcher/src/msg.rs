use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub hub_contract: HumanAddr,
    pub bluna_reward_contract: HumanAddr,
    pub stluna_reward_denom: String,
    pub bluna_reward_denom: String,
    pub lido_fee_address: HumanAddr,
    pub lido_fee_rate: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    SwapToRewardDenom {
        bluna_total_mint_amount: Uint128,
        stluna_total_mint_amount: Uint128,
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
