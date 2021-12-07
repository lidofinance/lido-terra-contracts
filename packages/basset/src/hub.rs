use cosmwasm_std::{
    to_binary, Binary, CanonicalAddr, Coin, Decimal, Deps, QueryRequest, StdResult, Uint128,
    WasmQuery,
};
use cw20::Cw20ReceiveMsg;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(PartialEq)]
pub enum BondType {
    BLuna,
    StLuna,
    BondRewards,
}

pub type UnbondRequest = Vec<(u64, Uint128, Uint128)>;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub epoch_period: u64,
    pub underlying_coin_denom: String,
    pub unbonding_period: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub reward_denom: String,
}

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
pub enum ExecuteMsg {
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

    /// Pauses the contracts. Only the owner or allowed guardians can pause the contracts
    PauseContracts {},

    /// Unpauses the contracts. Only the owner allowed to unpause the contracts
    UnpauseContracts {},

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
        redelegations: Vec<(String, Coin)>, //(dst_validator, amount)
    },

    /// Adds a list of addresses to a whitelist of guardians which can pause (but not unpause) the contracts
    AddGuardians {
        addresses: Vec<String>,
    },

    /// Removes a list of a addresses from a whitelist of guardians which can pause (but not unpause) the contracts
    RemoveGuardians {
        addresses: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Cw20HookMsg {
    Unbond {},
    Convert {},
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Parameters {
    pub epoch_period: u64,
    pub underlying_coin_denom: String,
    pub unbonding_period: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
    pub reward_denom: String,
    pub paused: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatch {
    pub id: u64,
    pub requested_bluna_with_fee: Uint128,
    pub requested_stluna: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct OldCurrentBatch {
    pub id: u64,
    pub requested_with_fee: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondHistory {
    pub batch_id: u64,
    pub time: u64,
    pub bluna_amount: Uint128,
    pub bluna_applied_exchange_rate: Decimal,
    pub bluna_withdraw_rate: Decimal,

    pub stluna_amount: Uint128,
    pub stluna_applied_exchange_rate: Decimal,
    pub stluna_withdraw_rate: Decimal,

    pub released: bool,
}

#[derive(JsonSchema, Serialize, Deserialize, Default)]
pub struct UnbondWaitEntity {
    pub bluna_amount: Uint128,
    pub stluna_amount: Uint128,
}

pub enum UnbondType {
    BLuna,
    StLuna,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct StateResponse {
    pub bluna_exchange_rate: Decimal,
    pub stluna_exchange_rate: Decimal,
    pub total_bond_bluna_amount: Uint128,
    pub total_bond_stluna_amount: Uint128,
    pub last_index_modification: u64,
    pub prev_hub_balance: Uint128,
    pub last_unbonded_time: u64,
    pub last_processed_batch: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
    pub reward_dispatcher_contract: Option<String>,
    pub validators_registry_contract: Option<String>,
    pub bluna_token_contract: Option<String>,
    pub stluna_token_contract: Option<String>,
    pub airdrop_registry_contract: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct CurrentBatchResponse {
    pub id: u64,
    pub requested_bluna_with_fee: Uint128,
    pub requested_stluna: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct WithdrawableUnbondedResponse {
    pub withdrawable: Uint128,
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct UnbondRequestsResponse {
    pub address: String,
    pub requests: UnbondRequest,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct AllHistoryResponse {
    pub history: Vec<UnbondHistory>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    Config {},
    State {},
    CurrentBatch {},
    WithdrawableUnbonded {
        address: String,
    },
    Parameters {},
    UnbondRequests {
        address: String,
    },
    AllHistory {
        start_from: Option<u64>,
        limit: Option<u32>,
    },
    Guardians,
}

pub fn is_paused(deps: Deps, hub_addr: String) -> StdResult<bool> {
    let params: Parameters = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: hub_addr,
        msg: to_binary(&QueryMsg::Parameters {})?,
    }))?;

    Ok(params.paused.unwrap_or(false))
}
