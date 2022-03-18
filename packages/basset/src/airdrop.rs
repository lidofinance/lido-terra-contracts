use cosmwasm_std::Decimal;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub hub_contract: String,
    pub reward_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::upper_case_acronyms)]
pub enum ExecuteMsg {
    UpdateConfig {
        owner: Option<String>,
        hub_contract: Option<String>,
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
pub struct AirdropInfo {
    pub airdrop_token_contract: String,
    pub airdrop_contract: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoOld {
    pub airdrop_token_contract: String,
    pub airdrop_contract: String,
    pub airdrop_swap_contract: String,
    pub swap_belief_price: Option<Decimal>,
    pub swap_max_spread: Option<Decimal>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub hub_contract: String,
    pub airdrop_tokens: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoElem {
    pub airdrop_token: String,
    pub info: AirdropInfo,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoElemOld {
    pub airdrop_token: String,
    pub info: AirdropInfoOld,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AirdropInfoResponse {
    pub airdrop_info: Vec<AirdropInfoElem>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
