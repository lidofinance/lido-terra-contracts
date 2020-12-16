use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InitMsg {
    pub epoch_time: u64,
    pub underlying_coin_denom: String,
    pub undelegated_epoch: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub reward_denom: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    ExchangeRate {},
    WhitelistedValidators {},
    WithdrawableUnbonded { address: HumanAddr, block_time: u64 },
    TokenContract {},
    RewardContract {},
    Parameters {},
    TotalBonded {},
    UnbondRequests { address: HumanAddr },
    UnbondEpochs { address: HumanAddr },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ExchangeRateResponse {
    pub rate: Decimal,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WhitelistedValidatorsResponse {
    pub validators: Vec<HumanAddr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct TotalBondedResponse {
    pub total_bonded: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WithdrawableUnbondedResponse {
    pub withdrawable: Uint128,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondRequestsResponse {
    pub unbond_requests: Vec<Uint128>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondEpochsResponse {
    pub unbond_epochs: Vec<u64>,
}
