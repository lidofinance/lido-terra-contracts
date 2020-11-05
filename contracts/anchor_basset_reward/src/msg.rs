use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{HumanAddr, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    SendReward {
        recipient: Option<HumanAddr>,
    },
    //Swap all of the balances to uusd.
    Swap {},
    //Update the global index
    UpdateGlobalIndex {
        past_balance: Uint128,
    },
    //Register bluna holders
    UpdateUserIndex {
        address: HumanAddr,
        is_send: Option<Uint128>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TokenInfoResponse {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    AccruedRewards { address: HumanAddr },
    GetIndex {},
    GetUserIn { address: HumanAddr },
}
