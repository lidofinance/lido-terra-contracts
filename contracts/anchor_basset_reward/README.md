# Anchor bAsset Reward <!-- omit in toc -->

The reward contract functionality is to send the reward to the ClaimReward sender. 
The governance contract instantiates the reward contract during its initialization. 

The reward contract is responsible for two main jobs:

- **_Send_** the reward in `uUSD` to the sender of the ClaimReward message.

- **_SWAP_** all rewards with different currencies to `uUSD`.

## Initialization

The reward contract is instantiated by governance. The instantiation is supposed to register the owner of the reward contract.

The instantiation is supposed to register the owner of the reward contract. The instantiation also sends `Register` [message](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_gov#register) to the governance contract. 

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
    // Send the reward to the user 
    // who has sent ClaimReward to governance contract.
    SendReward {
        receiver: HumanAddr,
        amount: Uint128,
    },
    //Swap all of the balances to uusd.
    Swap {},
}
```

### SendReward
- SendReward {*HumanAddr* receiver, Uint128 amount}
    * `receiver` is the receiver of the reward. `receiver` is the one who sends the `ClaimReward` to the governance contract.
    * `amount` is the reward amount that the `receiver` should accrued.
    * Verifies the sender of the message.
    * Sends the `amount` in `uUSD` to the user.

### Swap 
- Swap{}
    * This message should only be sent by the governance contract.
    * swaps all balances to `uUSD`




