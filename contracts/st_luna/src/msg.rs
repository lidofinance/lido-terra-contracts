use cosmwasm_std::HumanAddr;
use cw20::{Cw20CoinHuman, MinterResponse};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct TokenInitMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<Cw20CoinHuman>,
    pub mint: Option<MinterResponse>,
    pub hub_contract: HumanAddr,
}
