# Anchor bAsset Rewards Dispatcher  <!-- omit in toc -->

The Rewards Dispatcher contract accumulates the rewards from Hub's delegations and manages the rewards.
All rewards from *stLuna* tokens (the share of all rewards proportional to the amount of *stLuna* tokens minteds) are converted to Luna and are re-delegated back to the validators pool.
All rewards from *bLuna* (the share of all rewards proportional to the amount of *bLuna* tokens minted) are sended to the Reward Contract and handled the old way.
