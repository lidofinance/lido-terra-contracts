use crate::state::AirdropInfo;
use cosmwasm_std::{HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub hub_contract: HumanAddr,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    FabricateMIRClaim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
    UpdateConfig {
        owner: Option<HumanAddr>,
        hub_contract: Option<HumanAddr>,
    },
    AddAirdropToken {
        airdrop_token: String,
        airdrop_info: AirdropInfo,
    },
    RemoveAirdropToken {
        airdrop_token: String,
    },
    UpdateAirdropToken {
        airdrop_token: String,
        airdrop_info: AirdropInfo,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    AirdropInfo { airdrop_token: String },
    AirdropInfos {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub enum MIRAirdropHandleMsg {
    Claim {
        stage: u8,
        amount: Uint128,
        proof: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: HumanAddr,
    pub hub_contract: HumanAddr,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoResponse {
    pub airdrop_token_contract: HumanAddr,
    pub airdrop_contract: HumanAddr,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfosResponse {
    pub infos: Vec<AirdropInfo>,
}
