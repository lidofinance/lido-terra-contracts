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
    ////////////////////
    /// User's operations
    ////////////////////
    /// Receives `amount` Luna from sender.
    /// Delegate `amount` to a specific `validator`.
    /// Issue the same `amount` of bLuna to sender.
    Bond { validator: HumanAddr },

    ////////////////////
    /// User's operations
    ////////////////////
    /// Update global index
    UpdateGlobalIndex {},

    ////////////////////
    /// User's operations
    ////////////////////
    /// WithdrawUnbonded is suppose to ask for liquidated luna
    WithdrawUnbonded {},

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// Register receives the reward contract address
    RegisterSubcontracts { contract: Registration },

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// Register receives the reward contract address
    RegisterValidator { validator: HumanAddr },

    ////////////////////
    /// Owner's operations
    ////////////////////
    // Remove the validator from validators whitelist
    DeregisterValidator { validator: HumanAddr },

    /// (internal) Receive interface for send token
    Receive(Cw20ReceiveMsg),

    ////////////////////
    /// User's operations
    ////////////////////
    /// check whether the slashing has happened or not
    CheckSlashing {},

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// update the parameters that is needed for the contract
    UpdateParams {
        epoch_time: Option<u64>,
        underlying_coin_denom: Option<String>,
        undelegated_epoch: Option<u64>,
        peg_recovery_fee: Option<Decimal>,
        er_threshold: Option<Decimal>,
        reward_denom: Option<String>,
    },

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// switch of the message
    DeactivateMsg { msg: Deactivated },
    ////////////////////
    /// Owner's operations
    ////////////////////
    /// set the owener
    UpdateConfig { owner: HumanAddr },
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
    Unbond,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Unbond {},
}
