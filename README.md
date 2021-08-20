# Anchor bAsset Contracts

This monorepository contains the source code for the smart contracts implementing bAsset Protocol on the [Terra](https://terra.money) blockchain.

You can find information about the architecture, usage, and function of the smart contracts on the official Anchor documentation [site](https://anchorprotocol.com/).


## Contracts
| Contract                                            | Reference                                              | Description                                                                                                                        |
| --------------------------------------------------- | ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| [`anchor_basset_hub`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_hub)|[doc](https://docs.anchorprotocol/contracts/anchor_basset_hub)| Manages minted bLunas and bonded Lunas
| [`anchor_basset_reward`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_reward)|[doc](https://docs.anchorprotocol/contracts/anchor_basset_reward)|Manages the distribution of delegation rewards
| [`anchor_basset_token`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_token)| [doc](https://docs.anchorprotocol/contracts/anchor_basset_token)|CW20 compliance
| [`anchor_airdrop_registery`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_airdrop_registry)| [doc](https://docs.anchorprotocol/contracts/anchor_basset_airdrop_registery)|Manages message fabricators for MIR and ANC airdrops
| [`anchor_basset_rewards_dispatcher`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_rewards_dispatcher)| [doc](https://docs.anchorprotocol/contracts/anchor_basset_airdrop_registery)|Accumulates the rewards from Hub's delegations and manages the rewards
| [`st_luna`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/st_luna)| [doc](https://docs.anchorprotocol/contracts/anchor_basset_airdrop_registery)|CW20 compliance for stluna
| [`validators-registry`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/validators-registry)| [doc](https://docs.anchorprotocol/contracts/anchor_basset_airdrop_registery)|Approved validators whitelist
## Development

### Environment Setup

- Rust v1.44.1+
- `wasm32-unknown-unknown` target
- Docker

1. Install `rustup` via https://rustup.rs/

2. Run the following:

```sh
rustup default stable
rustup target add wasm32-unknown-unknown
```

3. Make sure [Docker](https://www.docker.com/) is installed

### Unit / Integration Tests

Each contract contains Rust unit tests embedded within the contract source directories. You can run:

```sh
cargo test unit-test
cargo test integration-test
```

### Compiling

After making sure tests pass, you can compile each contract with the following:

```sh
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/cw1_subkeys.wasm .
ls -l cw1_subkeys.wasm
sha256sum cw1_subkeys.wasm
```

#### Production

For production builds, run the following:

```sh
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/rust-optimizer:0.11.4
```

This performs several optimizations which can significantly reduce the final size of the contract binaries, which will be available inside the `artifacts/` directory.

# bLuna/stLuna Roadmap

### Overview (current)

The current *bLuna* design consists of three contracts:

1. Hub (the main entrypoint for all interactions except for claiming the rewards);
2. Cw20 Token (keeps track of the minted shares);
3. Rewards (users come here to claim their rewards in Terra USD);
4. There's no rewards fee.


![](https://i.imgur.com/1nVZAEg.png)

### Target state

1. The user can not delegate to a specific validator. A new contract is added, the Validators Registry, that keeps the list of approved validators and picks the next most sutable validator to delegate to. This aims to distribute the stake evenly across all validators.
2. A new token is added, *stLuna*. The token is accounted by a new (additional) contract. Note that *stLuna* balace is fixed (i.e., token balance equals the number of shares burned).
I.e., *stLuna* represents a (non-normalized) share of total staked Luna in that contract. E.g. if a person has 1 *stLuna* and total *stLuna* supply is 50, he has  *1 / 50 = 2%* share of all the staked Luna in that contract. If the total staked luna in contract is 100000, his share represents 2000 Luna staked;
2. The user can choose whether they want to mint *bLuna* or *stLuna* tokens when submitting his assets to the hub;
3. A new contact is added: the *Rewards Dispatcher*. It accumulates the rewards from Hub's delegations and manages the rewards;
4. All rewards from *stLuna* tokens (the share of all rewards proportional to the amount of *stLuna* tokens minteds) are converted to Luna and are re-delegated back to the validators pool;
5. All rewards from *bLuna* (the share of all rewards proportional to the amount of *bLuna* tokens minted) are hadled the old way;
6. An *x%* Lido rewards fee is added;
7. *stLuna* can be easily exchanged to *bLuna* and back, without breaking the existing *bLuna* rewards mechainics or restaking.

![](https://i.imgur.com/HvK4NpA.png)

### Roadmap

###### stLuna Token Contract Tasks

1. Create a vanilla Cw20 token (all methods are proxied to the underlying Cw20 implementation). ✓

###### Rewards Dispatcher Tasks

1. Create the contract stub (with mocks for *DispatchRewards* and *SwapToRewardDenom*); ✓
2. Implement the *DispatchRewards* handler. With the UST share of rewards, we do exactly what the Hub used to do — transfer to the bLuna Rewards contract, then *SwapToRewardDenom* and *UpdateGlobalIndex*. Then we call the Hub's updated *BondStLuna* method, adding the Luna share of rewards to the call. ✓
3. Implement the *SwapToRewardDenom* handler. This method should accept the *bLuna* and *stLuna*'s *total_amount_bonded* amounts as arguments (as received from the *Hub*). All reward coins (except for Luna and UST) are converted to UST, then the *stLuna* fraction is converted to Luna. ✓

###### Hub Tasks

1. Implement a separate *BondForStLuna* handler that bonds Luna for *stLuna*. We should introduce a separate *total_bond_amount_st_luna* and *total_bond_amount_b_luna* counter, along with a separate *update_exchange_rate_st_luna* mechanism to be able to calculate reward shares correctly; ✓
2. Implement a separate *BondRrewards* method for re-bonding rewards (add a sender address check — only the RewardDispatcher can do that). Note that such bonds do not require minting tokens; ✓
3. Redirect the rewards to the Rewards Dispatcher contract; ✓
4. Rewrite *UpdateGlobalIndex*: after calling Withdraw, the Hub first calls Rewards Dispatcher's *SwapToRewardDenom* method (providing the *bLuna* and *stLuna*'s *total_amount_bonded* amounts as arguments) and then calls Rewards Dispatcher's *DispatchRewards* method; ✓
5. Implement a separate *ReceiveStLuna* handler that burns *stLuna* for Luna; ✓
6. Implement correct slashing handling (promortional to the *stLuna/bLuna* — *total_bond_amount_st_luna* and *total_bond_amount_b_luna* should be updated); ✓
7. Migrate validator whitelists to Validator Registry;

###### Validator Registry Tasks

1. Implement the validator registry contract. ✓

## License

Copyright 2021 Anchor Protocol

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this file except in compliance with the License. You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0. Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.

See the License for the specific language governing permissions and limitations under the License.
