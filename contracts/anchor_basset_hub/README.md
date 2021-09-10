# Anchor bAsset Hub  <!-- omit in toc -->

**NOTE**: Reference documentation for this contract is available [here](https://anchor-protocol.gitbook.io/anchor/bluna/hub).

The Hub contract acts as the central hub for all minted bLuna and stLuna. Native Luna tokens received from users are delegated from here, and undelegations from bLuna/stLuna unbond requests are also handled from this contract. Rewards generated from delegations are withdrawn to the Rewards Dispatcher contract, bLuna portion of rewards distributed to bLuna holders and stLuna portion of rewards rebonded to hub.
