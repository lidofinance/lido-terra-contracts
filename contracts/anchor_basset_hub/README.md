# Anchor bAsset Governance <!-- omit in toc -->
This contract is supposed to manage bAsset in Terra blockchain. The contract consist of four core features:
    
   * A user can _***mint***_ bLuna by sending Luna to the contract, which results in the delegation of Luna.
   * A user can _***burn***_ bLuna by sending a transaction to undelegate and redeem underlying Luna.
   * A user can _***update_global_index***_ for calculating the global index as a scale for the reward distribution.
   
The governance contract is also supposed to manage the configs that are related to the whole bAsset smart contracts. Configs that are needed to be set up either through contract-to-contract interactions or creator-to-contract interaction are as follows:
* _***GovConfig***_: Stores the `owner` of the contract. 
* _***Parameters***_: Stores constant variable that the contract need for burn and mint messages. 
* _***WhiteListedValidators***_: Stores supported validators that the contract need to mint.
* _***MessagesStatus***_: Stores the status of messages. The status show whether the message is deactivated.


## State

### PoolInfo
PoolInfo manages reward distribution. It keeps the reward index as well as the total bonded amount to calculate the current reward index. Besides , PoolInfo Keeps the total supply of the contract to compute the exchange rate. 

```rust
pub struct PoolInfo {
    pub exchange_rate: Decimal,
    pub total_bond_amount: Uint128,
    pub claimed: Uint128,
    pub reward_index: Decimal,
    pub reward_account: CanonicalAddr,
}
```

### InitMsg
```rust
pub struct InitMsg {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    pub reward_code_id: u64,
    pub token_code_id: u64,
}
```
| Key                | Type       | 
| ------------------ | ---------- | 
| `name`     | String |
| `symbol`           | String    |
| `deceimals`        | u8    |
| `reward_code_id`           | u64    |
| `token_code_id`        | u64    |  

### Registration
Registration is an enum that specifies the sender of `RegisterSubContracts`.
If it is `Token`, it means the sender is token contract. If it is `Reward`, it means the sender is the reward contract.
```rust
pub enum Registration {
    Token,
    Reward,
}
```
### GovConfig
GovConfig stores the creator of the contract. The creator is necessary for setting up parameters and registering/deregistering validators.
```rust
pub struct GovConfig {
    pub creator: CanonicalAddr,
}
```
### Parameters
Parameters are general variables that the contract needs for couple of operations:
* `epoch_time`: determines the epoch window time period. For example, if it is 30, the contract collects all the `Receive` messages in 30-seconds time frame.
* `coin_denom`: determines the native coin the the contract is suppose to be driven from.
*  `undelegated_epoch`: determines the number of epochs that is equal to 21 days (undelegation period).
* `peg_recovery_fee`: determines the fee helping stabilize the exchange rate. 
* `er_threshold`: determines the threshold below of which the peg fee would need to be considered. 
```rust
pub struct Parameters {
    pub epoch_time: u64,
    pub coin_denom: String,
    pub undelegated_epoch: u64,
    pub peg_recovery_fee: Decimal,
    pub er_threshold: Decimal,
}
```
### MsgStatus
MsgState is designed as a switch to turn off a specific message. Slashing and Burn message is the only supported functions for the switch.
```rust
pub struct MsgStatus {
    pub slashing: Option<Deactivated>,
    pub burn: Option<Deactivated>,
}
```


## HandleMsg
```rust
pub enum HandleMsg {
    ////////////////////
    /// User's operations
    ////////////////////
    /// Receives `amount` Luna from sender.
    /// Delegate `amount` to a specific `validator`.
    /// Issue `amount * exchange_rate`  of bLuna to sender.
    Mint { validator: HumanAddr },

    ////////////////////
    /// User's operations
    ////////////////////
    /// Update global index for reward calculation.
    UpdateGlobalIndex {},

    ////////////////////
    /// User's operations
    ////////////////////
    /// FinishBurn is suppose to ask for liquidated luna
    FinishBurn {},

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// Register receives the reward and token contract address
    RegisterSubContracts { contract: Registration },

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// Register receives the reward contract address
    RegisterValidator { validator: HumanAddr },

    ////////////////////
    /// Owner's operations
    ////////////////////
    // Remove the validator from validators whitelist
    DeRegisterValidator { validator: HumanAddr },

    /// (internal) Receive interface for send token
    Receive(Cw20ReceiveMsg),

    ////////////////////
    /// User's operations
    ////////////////////
    /// check whether the slashing has happened or not
    ReportSlashing {},

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// update the parameters that is needed for the contract
    UpdateParams {
        epoch_time: u64,
        coin_denom: String,
        undelegated_epoch: u64,
        peg_recovery_fee: Decimal,
        er_threshold: Decimal,
    },

    ////////////////////
    /// Owner's operations
    ////////////////////
    /// switch of the message
    DeactivateMsg { msg: Deactivated },
}
```
### Mint
* Mint{*HumanAddr* validator}

    * The `amount` of Luna that the sender wants to delegate should be send along with the transaction as a coin, for example: `100uluna`.
    * `validator`: the `HumanAddr` of a whitelisted validator.
    *  `amount` Luna will be delegated to the validator.
    * Sends `Mint` message to the token contract. `amount * exchange_rate * (1 - peg_recovery_fee)` will be issued as a basset for the sender of the message in the token contract.


### UpdateGlobalIndex
* UpdateGlobalIndedx{}
    * Withdraws all rewards from all validators and sends them to the reward contract.
    * Send `Swap` message to the reward contract.
    * Send `UpdateGlobalIndex` to the reward contract.
    

### Receiver
The counter-part to `Send` is `Receive`, which must be implemented by
any contract that wishes to manage CW20 tokens. This is generally *not*
implemented by any CW20 contract.

`Receive{sender, amount, msg}` - This is designed to handle `Send`
messages. The address of the contract is stored in `env.sender`
so it cannot be faked. The contract should ensure the sender matches
the token contract it expects to handle, and not allow arbitrary addresses.

The `sender` is the original account requesting to move the tokens
and `msg` is a `Binary` data that can be decoded into a contract-specific
message. This can be empty if we have only one default action, 
or it may be a `ReceiveMsg` variant to clarify the intention. For example,
if I send to a uniswap contract, I can specify which token I want to swap 
against using this field.

```rust
pub enum Cw20HookMsg {
    InitBurn {},
}
```
* Receive(*Cw20ReceiveMsg* msg)
    * `msg` must be `Cw20HookMsg::InitBurn`.
    * Called by token contract.
    * In order to burn bassets, a user needs to send the `amount` to the governance contract.
    * Sending the basset to the smart contract will trigger the `InitBurn`.
    * The governance sends `Burn` message to the token contract.
    * The governance stores the request for the specific `epoch_id`.
        * The governance stores the request in `undelegated_wait_list`.
    * If the message is send when the epoch is passed, the governance contract sends `Undelegate` message to a random validator or validators. 
        * 'Undelegate' message includes the summation of all burn requests.
        * The governance contract applys both `exchange_rate` and `peg_fee_recovery`. 

### FinishBurn
* FinishBurn{}
    * Checks whether the unbonding period is over.
    * Updates `undelegated_wait_list`.
    * Sends amount Luna to sender.
        * If the user sends some `InitBurn` messages before unbonding period, the message pays back the summation of them.


        
### RegisterSubContracts { contract: Registration }
* RegisterSubcontracts(*Registration* contract)
    * contract is either `Token` or `Reward`.
    * Registering the instantiated reward/token contract on PoolInfo. 
    * This message is sent only once by the reward and token contracts during the initialization of the governance contract.


### RegisterValidator
- RegisterValidator {*HumanAddr* validator}
    - Checks whether the validator is a valid chain validator.
    - `validator` is the `HumanAddr` of a valid validator
    - Only the initializer of the governance contract can send this message.
    

## QueryMsg
```rust
pub enum QueryMsg {
    ExchangeRate {},
    WhiteListedValidators {},
    WithdrawableUnbonded { address: HumanAddr },
    GetToken {},
    GetReward {},
    GetParams {},
}
```

### ExchangeRate
* ExchangeRate {}
    * Returns the current `exchange_rate` of `PoolInfo`.

### WhiteListedValidators
* WhiteListedValidators {}
    * Returns all whitelisted validators.


### WithdrawableUnbonded
* WithdrawableUnbonded { *HumanAddr* : address}
    * `address`: the `HumanAddr` of the bluna user.
    * Returns possible withdrawable amount of `uluna` form the `address`.
    
### GetToken
* GetToke {}
    * Returns the address of the token contract.
    
### GetReward
* GetReward {}
    * Returns the address of the reward contract.
    
### GetParams
* GetParams {}
    * Returns the stored `Prameters`.
     