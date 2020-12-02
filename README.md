# Anchor bAsset Contracts

This repository contains smart contracts for bLuna. For more information about bAsset (bLuna), you can visit the official white paper [here](https://anchorprotocol.com/docs/The_bAsset_Protocol.pdf).

## Contracts


| Name                                                         | Description                      |
| ------------------------------------------------------------ | -------------------------------- |
| [`anchor_basset_hub`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_hub/README.md) | control governance               |
| [`anchor_basset_reward`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_reward/README.md) | control reward distribution               |
| [`anchor_basset_token`](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/tree/master/contracts/anchor_basset_token/README.md) | CW20 compliance |

## Initialization

For initializing anchor bAsset contracts, the initialization is only for `anchor_basset_gov`. `anchor_basset_reward` and `anchor_basset_token` will be instantiated from the `anchor_basset_gov`contract.

## Environment Setup

Contracts requires Rust version v1.44.1+ to build. Using [rustup](https://rustup.rs/) is recommended.

## Integration Tests
`anchor_basset_gov` contains a set of integration tests. To test, run the following:
 
```
cargo test
```

## Compiling
To compile all the contracts, run the following in the repo root:
```
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.10.4
```
This perform some optimizations that reduce the final size of contracts binaries. You can see the result inside the `artifacts/` directory.


## License
This software is licensed under the Apache 2.0 license. Read more about it [here](https://github.com/Anchor-Protocol/anchor-bAsset-contracts/blob/master/LICENSE)

© 2020 Terraform Labs, PTE.