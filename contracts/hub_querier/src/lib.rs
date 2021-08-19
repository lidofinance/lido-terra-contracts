use cosmwasm_std::{Binary, CanonicalAddr, Coin, Decimal, Uint128};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct State {
    pub bluna_exchange_rate: Decimal,
    pub stluna_exchange_rate: Decimal,
    pub total_bond_bluna_amount: Uint128,
    pub total_bond_stluna_amount: Uint128,
    pub last_index_modification: u64,
    pub prev_hub_balance: Uint128,
    pub last_unbonded_time: u64,
    pub last_processed_batch: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct OldState {
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
    pub reward_dispatcher_contract: Option<CanonicalAddr>,
    pub validators_registry_contract: Option<CanonicalAddr>,
    pub bluna_token_contract: Option<CanonicalAddr>,
    pub stluna_token_contract: Option<CanonicalAddr>,
    pub airdrop_registry_contract: Option<CanonicalAddr>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OldConfig {
    pub creator: CanonicalAddr,
    pub reward_contract: Option<CanonicalAddr>,
    pub token_contract: Option<CanonicalAddr>,
    pub airdrop_registry_contract: Option<CanonicalAddr>,
}

impl State {
    pub fn update_bluna_exchange_rate(
        &mut self,
        total_issued: Uint128,
        requested_with_fee: Uint128,
    ) {
        let actual_supply = total_issued + requested_with_fee;
        if self.total_bond_bluna_amount.is_zero() || actual_supply.is_zero() {
            self.bluna_exchange_rate = Decimal::one()
        } else {
            self.bluna_exchange_rate =
                Decimal::from_ratio(self.total_bond_bluna_amount, actual_supply);
        }
    }

    pub fn update_stluna_exchange_rate(&mut self, total_issued: Uint128, requested: Uint128) {
        let actual_supply = total_issued + requested;
        if self.total_bond_stluna_amount.is_zero() || actual_supply.is_zero() {
            self.stluna_exchange_rate = Decimal::one()
        } else {
            self.stluna_exchange_rate =
                Decimal::from_ratio(self.total_bond_stluna_amount, actual_supply);
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
        owner: Option<String>,
        rewards_dispatcher_contract: Option<String>,
        validators_registry_contract: Option<String>,
        bluna_token_contract: Option<String>,
        stluna_token_contract: Option<String>,
        airdrop_registry_contract: Option<String>,
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
    /// Delegate `amount` equally between validators from the registry.
    /// Issue `amount` / exchange_rate for the user.
    Bond {},

    BondForStLuna {},

    BondRewards {},

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
        airdrop_token_contract: String, // Contract address of MIR Cw20 Token
        airdrop_contract: String,       // Contract address of MIR Airdrop
        airdrop_swap_contract: String,  // E.g. Contract address of MIR <> UST Terraswap Pair
        claim_msg: Binary,              // Base64-encoded JSON of MIRAirdropHandleMsg::Claim
        swap_msg: Binary,               // Base64-encoded string of JSON of PairHandleMsg::Swap
    },

    /// Swaps claimed airdrop tokens to UST through Terraswap & sends resulting UST to bLuna Reward contract
    SwapHook {
        airdrop_token_contract: String, // E.g. contract address of MIR Token
        airdrop_swap_contract: String,  // E.g. Contract address of MIR <> UST Terraswap Pair
        swap_msg: Binary,               // E.g. Base64-encoded JSON of PairHandleMsg::Swap
    },

    RedelegateProxy {
        // delegator is automatically set to address of the calling contract
        src_validator: String,
        dst_validator: String,
        amount: Coin,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Unbond {},
    Convert {},
}
