use cosmwasm_std::{CanonicalAddr, Decimal, HumanAddr, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub last_index_modification: u64,
    pub prev_hub_balance: Uint128,
    pub actual_unbonded_amount: Uint128,
    pub last_unbonded_time: u64,
    pub last_processed_batch: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub creator: CanonicalAddr,
    pub reward_contract: Option<CanonicalAddr>,
    pub token_contract: Option<CanonicalAddr>,
}

impl State {
    pub fn update_exchange_rate(&mut self, total_issued: Uint128, requested_with_fee: Uint128) {
        let actual_supply = total_issued + requested_with_fee;
        if self.total_bond_amount.is_zero() || actual_supply.is_zero() {
            self.exchange_rate = Decimal::one()
        } else {
            self.exchange_rate = Decimal::from_ratio(self.total_bond_amount, actual_supply);
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
    RegisterSubcontracts {
        contract: Registration,
        contract_address: HumanAddr,
    },

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
        epoch_period: Option<u64>,
        underlying_coin_denom: Option<String>,
        unbonding_period: Option<u64>,
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
    UpdateConfig {
        owner: Option<HumanAddr>,
        reward_contract: Option<HumanAddr>,
        token_contract: Option<HumanAddr>,
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
    Unbond,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Unbond {},
}
