#!/usr/local/bin/python3

import json
import os
from deploy_local import *
from terra_sdk.core.wasm import MsgMigrateContract
import sys

TERRA_ORIGINAL_HUB_CONTRACT = "anchor_basset_hub.wasm"
TERRA_ORIGINAL_REWARD_CONTRACT = "anchor_basset_reward.wasm"
TERRA_ORIGINAL_TOKEN_CONTRACT = "anchor_basset_token.wasm"


def migrate_contract(terra_client, wallet, contract_address, new_code_id, message):
    print("Migrating {0} to new code_id = {1}...".format(contract_address, new_code_id))

    execute = MsgMigrateContract(
        wallet.key.acc_address,
        contract_address,
        new_code_id,
        message,
    )

    migrate_tx = wallet.create_and_sign_tx(
        msgs=[execute], fee=StdFee(1000000, Coins(uluna=1000000))
    )

    migrate_tx_result = terra_client.tx.broadcast(migrate_tx)
    if migrate_tx_result.code is not None:
        print("Error while migrating contract: {}".format(migrate_tx_result.raw_log))
        exit(1)

    return migrate_tx_result


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Please provide a path to the original Terra contract's folder")
        exit(1)

    terra = LocalTerra()
    test1 = terra.wallets["test1"]

    print("Uploading original Terra contracts...")

    TERRA_ORIGINAL_CONTRACTS_FOLDER = sys.argv[1]

    original_hub_code_id = store_code(terra, test1, os.path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER,
                                                                 TERRA_ORIGINAL_HUB_CONTRACT))
    original_reward_code_id = store_code(terra, test1, os.path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER,
                                                                    TERRA_ORIGINAL_REWARD_CONTRACT))
    original_bluna_token_code_id = store_code(terra, test1, os.path.join(TERRA_ORIGINAL_CONTRACTS_FOLDER,
                                                                         TERRA_ORIGINAL_TOKEN_CONTRACT))

    hub_address = instantiate_contract(terra, test1, original_hub_code_id,
                                       {"epoch_period": 30, "er_threshold": "10000000000000", "peg_recovery_fee": "0",
                                        "reward_denom": "uusd", "unbonding_period": 2, "underlying_coin_denom": "uluna",
                                        "validator": "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5"},
                                       Coins(uluna=1000000))

    reward_address = instantiate_contract(terra, test1, original_reward_code_id,
                                          {"hub_contract": hub_address, "reward_denom": "uusd"})

    bluna_token_address = instantiate_contract(terra, test1, original_bluna_token_code_id,
                                               {"decimals": 6, "hub_contract": hub_address,
                                                "initial_balances": [{"address": hub_address, "amount": "1000000"}],
                                                "name": "bluna", "symbol": "BLUNA",
                                                "mint": {"minter": hub_address, "cap": None}})

    update_hub_config = execute_contract(terra, test1, hub_address, {
        "update_config": {"token_contract": bluna_token_address,
                          "reward_contract": reward_address}})

    print()
    print("Starting migration process...")

    new_hub_code_id = store_code(terra, test1, "artifacts/anchor_basset_hub.wasm")
    new_reward_code_id = store_code(terra, test1, "artifacts/anchor_basset_reward.wasm")
    new_bluna_token_code_id = store_code(terra, test1, "artifacts/anchor_basset_token.wasm")
    rewards_dispatcher_code_id = store_code(terra, test1, "artifacts/anchor_basset_rewards_dispatcher.wasm")
    validators_registry_code_id = store_code(terra, test1, "artifacts/anchor_basset_validators_registry.wasm")
    stluna_token_code_id = store_code(terra, test1, "artifacts/anchor_basset_token_stluna.wasm")

    rewards_dispatcher_address = instantiate_contract(terra, test1, rewards_dispatcher_code_id,
                                                      {"lido_fee_address": test1.key.acc_address,
                                                       "lido_fee_rate": "0.05", "hub_contract": hub_address,
                                                       "bluna_reward_contract": reward_address,
                                                       "stluna_reward_denom": "uluna", "bluna_reward_denom": "uusd"})

    validators_registry_address = instantiate_contract(terra, test1, validators_registry_code_id,
                                                       {"hub_contract": hub_address,
                                                        "registry": []})

    stluna_token_address = instantiate_contract(terra, test1, stluna_token_code_id,
                                                {"decimals": 6, "hub_contract": hub_address, "initial_balances": [],
                                                 "name": "stluna", "symbol": "STLUNA",
                                                 "mint": {"minter": hub_address, "cap": None}})

    migrate_hub = migrate_contract(terra, test1, hub_address, new_hub_code_id, {
        "stluna_exchange_rate": "1.0",
        "total_bond_stluna_amount": "0",
        "reward_dispatcher_contract": rewards_dispatcher_address,
        "validators_registry_contract": validators_registry_address,
        "stluna_token_contract": stluna_token_address
    })
    migrate_rewards = migrate_contract(terra, test1, reward_address, new_reward_code_id, {})
    migrate_bluna_token = migrate_contract(terra, test1, bluna_token_address, new_bluna_token_code_id, {})

    print()
    print("HUB_CONTRACT = {}".format(hub_address))
    print("REWARD_CONTRACT = {}".format(reward_address))
    print("REWARDS_DISPATCHER_CONTRACT = {}".format(rewards_dispatcher_address))
    print("VALIDATORS_REGISTRY_CONTRACT = {}".format(validators_registry_address))
    print("BLUNA_TOKEN_CONTRACT = {}".format(bluna_token_address))
    print("STLUNA_TOKEN_CONTRACT = {}".format(stluna_token_address))

    # just a few tests to make sure the contracts works
    # for more accurate tests we must use integration-tests repo
    execute_contract(terra, test1, hub_address, {"bond_for_st_luna": {}}, Coins(uluna=1000000))
    execute_contract(terra, test1, hub_address, {"bond": {}}, Coins(uluna=1000000))

    execute_contract(terra, test1, hub_address, {"bond_for_st_luna": {}}, Coins(uluna=1000000))
    execute_contract(terra, test1, hub_address, {"bond": {}}, Coins(uluna=1000000))

    execute_contract(terra, test1, stluna_token_address, {"send": {"contract": hub_address, "amount": "1000000",
                                                                   "msg": base64.b64encode(
                                                                       json.dumps({"unbond": {}}).encode(
                                                                           'utf-8')).decode('utf-8')}})
    execute_contract(terra, test1, bluna_token_address, {"send": {"contract": hub_address, "amount": "1000000",
                                                                  "msg": base64.b64encode(
                                                                      json.dumps({"unbond": {}}).encode(
                                                                          'utf-8')).decode('utf-8')}})
