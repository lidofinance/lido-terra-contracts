# Anchor_bAsset <!-- omit in toc -->
This contract is supposed to manage bAsset in Terra blockchain. The contract consist of four core features:
    
   * A user can _***mint***_ bLuna by sending Luna to the contract, which results in the delegation of Luna.
   * A user can _***burn***_ bLuna by sending a transaction to undelegate and redeem underlying Luna.
   * A user can _***claim rewards***_ for bLuna by sending a transaction claiming staking rewards of the underlying Luna delegation.
   * A user can *_**send**_* bLuna to an another account.
   
## Configs 
| Name         | 
| ------------ | 
| name        |         
| symbol | 
|decimals|

## State
### TokenInfo
TokenInfo holds general information related to the contract. 
```rust
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub total_supply: Uint128,
}
```
### TokenState
TokenState is supposed to keep information related to current state of the token.

_current_epoc:_ the contract gathers all burn requests in 6 hours and manage them all at once. In order to do this, EpocId is designed. this variable keeps the current epoc of the contract.

_current_block_time:_  is used to help to calculate current epoc.

_delegation_map:_  maps addresses of validators that the contract has delegated to.

_holder_map_:  maps bLuna holdings and accrued rewards (index).

_undelegated_wait_list_ stores _**InitBurn**_ requests per each Epoc. Each epoc has an Identical EpocId.

```rust
pub struct TokenState {
    pub current_epoc: u64,
    pub current_block_time: u64,
    pub delegation_map: HashMap<HumanAddr, Uint128>,
    pub holder_map: HashMap<HumanAddr, Decimal>,
    pub undelegated_wait_list: HashMap<EpocId, Undelegation>,
}
```

### PoolInfo
PoolInfo manages reward distribution. It keeps the reward index as well as the total bonded amount to calculate the current reward index. Besides , PoolInfo Keeps the total supply of the contract to compute the exchange rate. 

```rust
pub struct PoolInfo {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub total_issued: Uint128,
    pub claimed: Uint128,
    pub reward_index: Decimal,
}
```

## InitMsg
```rust
pub struct InitMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}
```
| Key                | Type       | 
| ------------------ | ---------- | 
| `name`     | String |
| `symbol`           | String    |
| `deceimals`        | u8    | 

## HandleMsg
```rust
pub enum HandleMsg {
    /// Mint is a message to work as follows:
    /// Receives `amount` Luna from sender.
    /// Delegate `amount` to a specific `validator`.
    /// Issue the same `amount` of bLuna to sender.
    Mint {
        validator: HumanAddr,
        amount: Uint128,
    },
    /// ClaimRewards sends bluna rewards to sender.
    ClaimRewards {},
    /// InitBurn is send an undelegate message after receiving all
    /// requests for an specific period of time.
    InitBurn { amount: Uint128 },
    /// FinishBurn is suppose to ask for liquidated luna
    FinishBurn { amount: Uint128 },
    /// Send is like a base message in CW20 to move bluna to another account
    Send {
        recipient: HumanAddr,
        amount: Uint128,
    },
}
```
### Mint
* Mint{*address* validator, *Uint128* amount}

    * Receives amount Luna from sender.
    * amount Luna is delegated to validator.
    * Updates `token_state.undelagation_map`.
    * Updates `token_state.holding_map`.
        * Mints amount/exchangeRate bLuna to sender.
            * If `pool_info.exchange_rate` < 1, a 0.1% fee is attached.


### ClaimRewards
* ClaimRewards{}
    * Sends previously accrued bLuna rewards to sender.
    * Updates `token_state.holding_map`.
        * Updates previously recorded index values to the current rewardIndex.


### InitBurn
* InitiateBurn{*Uint128* amount}
    * Invokes ClaimRewards{}.
    * Updates `token_state.holding_map`.
        * Burns amount bLuna from sender.
    * Updates `Token_state.undelegation_wait_list_Map`.
    * If EpochTime has passed since last Undelegate{} execution, invokes Undelegate{}.

### FinishBurn
* FinishBurn{*Uint128* amount}
    * Checks whether the unbonding period is over.
    * Updates `token_state.undelegated_wait_list_map`.
    * Sends amount Luna to sender.


### Send
Sending bLuna to a different account automatically credits previously accrued rewards to the sender.

* Send{*address* recipient, *Uint128* amount}
    * Invokes ClaimRewards{}.
    * Updates `token_state.holding_map`.
        * Sends amount bLuna to recipient.

## QueryMsg
This will be provided. 