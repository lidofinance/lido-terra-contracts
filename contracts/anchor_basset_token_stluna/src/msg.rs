use cw20::{Cw20Coin, MinterResponse};
use cw20_base::msg::InstantiateMarketingInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct TokenInitMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub initial_balances: Vec<Cw20Coin>,
    pub mint: Option<MinterResponse>,
    pub hub_contract: String,
    pub marketing: Option<InstantiateMarketingInfo>,
}
