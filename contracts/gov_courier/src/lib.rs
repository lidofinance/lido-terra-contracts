use cosmwasm_std::{CanonicalAddr, Decimal, HumanAddr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct PoolInfo {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub last_index_modification: u64,
    pub reward_account: CanonicalAddr,
    pub is_reward_exist: bool,
    pub is_token_exist: bool,
    pub token_account: CanonicalAddr,
}

impl PoolInfo {
    pub fn update_exchange_rate(&mut self, total_issued: Uint128) {
        if self.total_bond_amount.is_zero() || total_issued.is_zero() {
            self.exchange_rate = Decimal::one()
        } else {
            self.exchange_rate = Decimal::from_ratio(self.total_bond_amount, total_issued);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HandleMsg {
    /// Mint is a message to work as follows:
    /// Receives `amount` Luna from sender.
    /// Delegate `amount` to a specific `validator`.
    /// Issue the same `amount` of bLuna to sender.
    Mint {
        validator: HumanAddr,
    },
    /// Update general index
    UpdateGlobalIndex {},
    /// FinishBurn is suppose to ask for liquidated luna
    FinishBurn {},
    // Register receives the reward contract address
    RegisterSubContracts {
        contract: Registration,
    },
    // Register receives the reward contract address
    RegisterValidator {
        validator: HumanAddr,
    },
    // Remove the validator from validators whitelist
    DeRegisterValidator {
        validator: HumanAddr,
    },
    //Receive interface for send token
    Receive(Cw20ReceiveMsg),
    //check whether the slashing has happened or not
    ReportSlashing {},
    //update the parameters that is needed for the contract
    UpdateParams {
        epoch_time: u64,
        coin_denom: String,
        undelegated_epoch: u64,
    },
    DeactivateMsg {
        msg: Deactivated,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Registration {
    Token,
    Reward,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Deactivated {
    Slashing,
    Burn,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    InitBurn {},
}
