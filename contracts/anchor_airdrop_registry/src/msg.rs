use crate::state::AirdropInfo;
use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub hub_contract: HumanAddr,
    pub reward_contract: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    FabricateMIRClaim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
    FabricateANCClaim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
    UpdateConfig {
        owner: Option<HumanAddr>,
        hub_contract: Option<HumanAddr>,
        reward_contract: Option<HumanAddr>,
    },
    AddAirdropInfo {
        airdrop_token: String,
        airdrop_info: AirdropInfo,
    },
    RemoveAirdropInfo {
        airdrop_token: String,
    },
    UpdateAirdropInfo {
        airdrop_token: String,
        airdrop_info: AirdropInfo,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    AirdropInfo {
        airdrop_token: Option<String>,
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MIRAirdropHandleMsg {
    Claim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ANCAirdropHandleMsg {
    Claim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairHandleMsg {
    Swap {
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<HumanAddr>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: HumanAddr,
    pub hub_contract: HumanAddr,
    pub reward_contract: HumanAddr,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoElem {
    pub airdrop_token: String,
    pub info: AirdropInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoResponse {
    pub airdrop_info: Vec<AirdropInfoElem>,
}
