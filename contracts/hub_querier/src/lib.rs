use cosmwasm_std::{Binary, CanonicalAddr, Decimal, HumanAddr, Uint128};
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
    pub airdrop_registry_contract: Option<CanonicalAddr>,
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
    /// Owner's operations
    ////////////////////

    /// Set the owener
    UpdateConfig {
        owner: Option<HumanAddr>,
        reward_contract: Option<HumanAddr>,
        token_contract: Option<HumanAddr>,
        airdrop_registry_contract: Option<HumanAddr>,
    },

    /// Register receives the reward contract address
    RegisterValidator {
        validator: HumanAddr,
    },

    // Remove the validator from validators whitelist
    DeregisterValidator {
        validator: HumanAddr,
    },

    /// update the parameters that is needed for the contract
    UpdateParams {
        epoch_period: Option<u64>,
        unbonding_period: Option<u64>,
        peg_recovery_fee: Option<Decimal>,
        er_threshold: Option<Decimal>,
    },

    ////////////////////
    /// User's operations
    ////////////////////

    /// Receives `amount` in underlying coin denom from sender.
    /// Delegate `amount` to a specific `validator`.
    /// Issue `amount` / exchange_rate for the user.
    Bond {
        validator: HumanAddr,
    },

    /// Update global index
    UpdateGlobalIndex {
        airdrop_hooks: Option<Vec<Binary>>,
    },

    /// Send back unbonded coin to the user
    WithdrawUnbonded {},

    /// Check whether the slashing has happened or not
    CheckSlashing {},

    ////////////////////
    /// bAsset's operations
    ///////////////////

    /// Receive interface for send token.
    /// Unbond the underlying coin denom.
    /// Burn the received basset token.
    Receive(Cw20ReceiveMsg),

    ////////////////////
    /// internal operations
    ///////////////////
    ClaimAirdrop {
        airdrop_token_contract: HumanAddr, // Contract address of MIR Cw20 Token
        airdrop_contract: HumanAddr,       // Contract address of MIR Airdrop
        airdrop_swap_contract: HumanAddr,  // E.g. Contract address of MIR <> UST Terraswap Pair
        claim_msg: Binary,                 // Base64-encoded JSON of MIRAirdropHandleMsg::Claim
        swap_msg: Binary,                  // Base64-encoded string of JSON of PairHandleMsg::Swap
    },

    /// Swaps claimed airdrop tokens to UST through Terraswap & sends resulting UST to bLuna Reward contract
    SwapHook {
        airdrop_token_contract: HumanAddr, // E.g. contract address of MIR Token
        airdrop_swap_contract: HumanAddr,  // E.g. Contract address of MIR <> UST Terraswap Pair
        swap_msg: Binary,                  // E.g. Base64-encoded JSON of PairHandleMsg::Swap
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Unbond {},
}
