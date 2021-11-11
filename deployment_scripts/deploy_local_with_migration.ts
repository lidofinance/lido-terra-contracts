import {Coins, LocalTerra} from "@terra-money/terra.js";
import {executeContract, instantiateContract, migrateContract, storeCode} from "./common";
const path = require('path');

const TERRA_ORIGINAL_HUB_CONTRACT = "anchor_basset_hub.wasm"
const TERRA_ORIGINAL_REWARD_CONTRACT = "anchor_basset_reward.wasm"
const TERRA_ORIGINAL_TOKEN_CONTRACT = "anchor_basset_token.wasm"

async function main(): Promise<void> {
  if (process.argv.length < 3) {
    throw new Error("Provide path to the original Terra contracts, please. ts-node deploy_local_with_migration.ts ~/original_contracts")
  }
  const terra = new LocalTerra();
  const {test1} = terra.wallets;

  console.log("Uploading original Terra contracts...")

  const TERRA_ORIGINAL_CONTRACTS_FOLDER = process.argv[2]

  let originalHubCodeId = await storeCode(terra, test1, path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER, TERRA_ORIGINAL_HUB_CONTRACT))
  let originalRewardCodeId = await storeCode(terra, test1, path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER, TERRA_ORIGINAL_REWARD_CONTRACT))
  let originalBlunaTokenCodeId = await storeCode(terra, test1, path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER, TERRA_ORIGINAL_TOKEN_CONTRACT))

  let hubAddress = await instantiateContract(terra, test1, originalHubCodeId,
    {epoch_period: 0, er_threshold: "1.0", peg_recovery_fee: "0", reward_denom: "uusd", unbonding_period: 0, underlying_coin_denom: "uluna", validator: "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5"}, new Coins({uluna: 1000000}))

  let rewardAddress = await instantiateContract(terra, test1, originalRewardCodeId,
    {hub_contract: hubAddress, reward_denom: "uusd"}, new Coins({}))

  let blunaTokenAddress = await instantiateContract(terra, test1, originalBlunaTokenCodeId,
    {decimals: 6, hub_contract: hubAddress, initial_balances: [{address: hubAddress, amount: "1000000"}],
      name: "bluna", symbol: "BLUNA",
      mint: {minter: hubAddress, cap: null}}, new Coins({}))

  await executeContract(terra, test1, hubAddress, {
    update_config: {token_contract: blunaTokenAddress, reward_contract: rewardAddress}}, new Coins({}))


  await executeContract(terra, test1, hubAddress, {bond: {validator: "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5"}}, new Coins({uluna: 1000000000}))

  // simulate large unbond queue from many users
  for (let i = 0; i < 31; i++ ) {
    await executeContract(terra, test1, blunaTokenAddress, {send: {contract: hubAddress, amount: "10000",
        msg: Buffer.from(JSON.stringify({"unbond": {}})).toString('base64')}}, new Coins({}));
    console.log("UNBOND REQUEST NUMBER #", i)
  }

  console.log()
  console.log("Starting migration process...")

  let newHubCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_hub.wasm")
  let newRewardCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_reward.wasm")
  let newBlunaTokenCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_token.wasm")
  let rewardsDispatcherCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_rewards_dispatcher.wasm")
  let validatorsRegistryCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_validators_registry.wasm")
  let stlunaTokenCodeId = await storeCode(terra, test1, "../artifacts/anchor_basset_token_stluna.wasm")

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

  console.log("Migrating hub...")
  await migrateContract(terra, test1, hubAddress, newHubCodeId, {
    reward_dispatcher_contract: rewardsDispatcherAddress,
    validators_registry_contract: validatorsRegistryAddress,
    stluna_token_contract: stlunaTokenAddress
  })
  console.log("Done")

  try {
    await executeContract(terra, test1, hubAddress, {withdraw_unbonded: {}}, new Coins({}));
  } catch (e) {
    // the hub is paused
    console.log("Error: ", e.response.data.error)
  }

  console.log("Migrating rewards...")
  await migrateContract(terra, test1, rewardAddress, newRewardCodeId, {})
  console.log("Done")

  console.log("Migrating bLuna token...")
  await migrateContract(terra, test1, blunaTokenAddress, newBlunaTokenCodeId, {})
  console.log("Done")

  for (let i = 0; i < 4; i++ ) {
    await executeContract(terra, test1, hubAddress, {migrate_unbond_wait_list: {limit: 10}}, new Coins({}));
  }

  await executeContract(terra, test1, hubAddress, {
    update_params: {paused: false}}, new Coins({}));

  console.log()

  console.log(`HUB_CONTRACT = ${hubAddress}`)
  console.log(`REWARD_CONTRACT = ${rewardAddress}`)
  console.log(`REWARDS_DISPATCHER_CONTRACT = ${rewardsDispatcherAddress}`)
  console.log(`VALIDATORS_REGISTRY_CONTRACT = ${validatorsRegistryAddress}"`)
  console.log(`BLUNA_TOKEN_CONTRACT = ${blunaTokenAddress}`)
  console.log(`STLUNA_TOKEN_CONTRACT = ${stlunaTokenAddress}`)


  //just a few simple tests to make sure the contracts are not failing
  //for more accurate tests we must use integration-tests repo
  await executeContract(terra, test1, hubAddress, {withdraw_unbonded: {}}, new Coins({}));

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
