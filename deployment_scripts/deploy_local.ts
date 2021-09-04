import {
  Coins, LocalTerra,
} from '@terra-money/terra.js';
import {executeContract, instantiateContract, storeCode} from "./common";

// Current contracts works with LocalTerra commit 1c3f42a60116b4c17cb5d002aa194eae9b8811b5 only (or older)
// Cause the latest LocalTerra uses Bombay network and the contracts are incompatible with that yet

async function main(): Promise<void> {
  const terra = new LocalTerra();
  const {test1} = terra.wallets;

  let hubCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_hub.wasm")
  let rewardCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_reward.wasm")
  let blunaTokenCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_token.wasm")
  let rewardsDispatcherCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_rewards_dispatcher.wasm")
  let validatorsRegistryCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_validators_registry.wasm")
  let stlunaTokenCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_token_stluna.wasm")

  console.log()

  let hubAddress = await instantiateContract(terra, test1, hubCodeId,
    {epoch_period: 30, er_threshold: "1.0", peg_recovery_fee: "0", reward_denom: "uusd", unbonding_period: 2, underlying_coin_denom: "uluna"}, new Coins({}))

  let rewardAddress = await instantiateContract(terra, test1, rewardCodeId,
    {hub_contract: hubAddress, reward_denom: "uusd"}, new Coins({}))

  let blunaTokenAddress = await instantiateContract(terra, test1, blunaTokenCodeId,
    {decimals: 6, hub_contract: hubAddress, initial_balances: [],
      name: "bluna", symbol: "BLUNA",
      mint: {minter: hubAddress, cap: null}}, new Coins({}))

  let rewardsDispatcherAddress = await instantiateContract(terra, test1, rewardsDispatcherCodeId,
    {lido_fee_address: test1.key.accAddress,
      lido_fee_rate: "0.05", hub_contract: hubAddress, bluna_reward_contract: rewardAddress,
      stluna_reward_denom: "uluna", bluna_reward_denom: "uusd"}, new Coins({}))

  let validatorsRegistryAddress = await instantiateContract(terra, test1, validatorsRegistryCodeId,
    {hub_contract: hubAddress,
      registry: [{active: true,
        address: "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5",
        total_delegated: "0"}]}, new Coins({}))

  let stlunaTokenAddress = await instantiateContract(terra, test1, stlunaTokenCodeId,
    {decimals: 6, hub_contract: hubAddress, initial_balances: [],
      name: "stluna", symbol: "STLUNA",
      mint: {minter: hubAddress, cap: null}}, new Coins({}))

  console.log()

  console.log("Updating hub's config...")

  await executeContract(terra, test1, hubAddress, {
    update_config: {bluna_token_contract: blunaTokenAddress, stluna_token_contract: stlunaTokenAddress,
      rewards_dispatcher_contract: rewardsDispatcherAddress,
      validators_registry_contract: validatorsRegistryAddress}}, new Coins({}))

  console.log()

  console.log(`HUB_CONTRACT = ${hubAddress}`)
  console.log(`REWARD_CONTRACT = ${rewardAddress}`)
  console.log(`REWARDS_DISPATCHER_CONTRACT = ${rewardsDispatcherAddress}`)
  console.log(`VALIDATORS_REGISTRY_CONTRACT = ${validatorsRegistryAddress}"`)
  console.log(`BLUNA_TOKEN_CONTRACT = ${blunaTokenAddress}`)
  console.log(`STLUNA_TOKEN_CONTRACT = ${stlunaTokenAddress}`)

  //just a few simple tests to make sure the contracts are not failing
  //for more accurate tests we must use integration-tests repo
  await executeContract(terra, test1, hubAddress, {bond_for_st_luna: {}}, new Coins({uluna: 1000000}))
  await executeContract(terra, test1, hubAddress, {bond: {}}, new Coins({uluna: 1000000}))

  await executeContract(terra, test1, hubAddress, {bond_for_st_luna: {}}, new Coins({uluna: 1000000}))
  await executeContract(terra, test1, hubAddress, {bond: {}}, new Coins({uluna: 1000000}))

  await executeContract(terra, test1, stlunaTokenAddress, {send: {contract: hubAddress, amount: "1000000",
      msg: Buffer.from(JSON.stringify({"unbond": {}})).toString('base64')}}, new Coins({}))

  await executeContract(terra, test1, blunaTokenAddress, {send: {contract: hubAddress, amount: "1000000",
      msg: Buffer.from(JSON.stringify({"unbond": {}})).toString('base64')}}, new Coins({}))

}

main().catch(console.log);
