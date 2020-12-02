# Anchor bAsset Reward <!-- omit in toc -->

The reward contract functionality is to send the reward to the ClaimReward sender. 
The governance contract instantiates the reward contract during its initialization. 

The reward contract is responsible for two main jobs:

- **_ClaimRewad_** sends back the rewards in `uUSD` to the sender of the ClaimReward message.
- **_SWAP_** all rewards with different currencies to `uUSD`.
- **_UpdateGlobalIndex_** calculates the global index.
- **_UpdateUserIndex_** updates the user index and calculates the reward of the user.


## Initialization


The instantiation is supposed to register the owner of the reward contract. The instantiation also sends `Register` [message](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_hub#RegisterSubContracts) to the governance contract. 

## State
 ``` rust
 pub struct Config {
     // The owner have to be the governance contract
     pub owner: CanonicalAddr,
 }
```

## HandleMsg
```rust
pub enum HandleMsg {
    ////////////////////
    /// User's operations
    ///////////////////
    /// return the accrued reward in uusd to the user.
    ClaimReward { recipient: Option<HumanAddr> },

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Swap all of the balances to uusd.
    Swap {},

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Update the global index
    UpdateGlobalIndex {},

    ////////////////////
    /// Owner's operations
    ///////////////////
    //Register bluna holders
    UpdateUserIndex {
        address: HumanAddr,
        is_send: Option<Uint128>,
    },
}
```

### Swap 
- Swap{}
    * This message should only be sent by the governance contract.
    * swaps all balances to `uUSD`

### ClaimRewards
* ClaimRewards{*recipient* `Option<HumanAddr>`}
    * The receiver of the ClaimReward is specified by `recipient`.
    * If to is `None`, the receiver is the sender.
    * Sends previously accrued bLuna rewards to sender.
    * Updates user index.
        * Updates previously recorded index values to the current rewardIndex.

### UpdateGlobalIndex
* UpdateGlobalIndex{}
   * Must be send only by the `governance` contract.
   * Calculates the global index based on `newly_added_rewards/total_issue`.
   
### UpdateUserIndex
* UpdateUserIndex{*address* `HumanAddr`, *is_send* `Option<Uint128>`}
   * Must be send by either the governance contract or the token contract.
   * If *is_send* is `None`, it updates the user index to the global index.
        * Else, it update the user index to the global index, and sends the accrued reward of the user to its own `pending_reward` state.
        
## QueryMsg
```
pub enum QueryMsg {
    AccruedRewards { address: HumanAddr },
    GetIndex {},
    GetUserIndex { address: HumanAddr },
    GetPending { address: HumanAddr },
}
```

### AccruedRewards
* AccruedRewards { *HumanAddr* : address}
     * `address`: the `HumanAddr` of the bluna user.
     * Return the expected reward of the `address`.

###  GetIndex
*  GetIndex {}
    * Returns the global index.
    
### GetUserIndex
* GetUserIndex { address: HumanAddr }
    * Returns the index of the specified address.
    
### GetPending
*  GetPending { address: HumanAddr }
    * Returns the pending reward of the spcified address.