use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{HumanAddr, Uint128};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    ////////////////////
    /// User's operations
    ///////////////////
    /// return the accrued reward in uusd to the user.
    ClaimReward { recipient: Option<HumanAddr> },

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Swap all of the balances to uusd.
    Swap {},

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Update the global index
    UpdateGlobalIndex {},

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Register bluna holders
    UpdateUserIndex {
        address: HumanAddr,
        is_send: Option<Uint128>,
    },

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Register bluna holders
    UpdateParams { swap_denom: String },
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
    GetUserIndex { address: HumanAddr },
    GetPending { address: HumanAddr },
}
