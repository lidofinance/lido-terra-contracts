use crate::state::EpochId;
use cosmwasm_std::{Decimal, HumanAddr, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type UnbondRequest = Vec<(u64, Uint128)>;
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
    CurrentEpoch {},
    CollectedInEpoch { epoch_id: u64 },
    LastIndexModification {},
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
    pub address: HumanAddr,
    pub requests: UnbondRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondEpochsResponse {
    pub unbond_epochs: Vec<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CollectedInEpochResponse {
    pub epoch_id: u64,
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentEpochResponse {
    pub epoch_id: EpochId,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct LastIndexModificationResponse {
    pub time: u64,
}
