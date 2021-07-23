#!/usr/local/bin/python3

import base64
from terra_sdk.client.localterra import LocalTerra
from terra_sdk.core.wasm import MsgStoreCode, MsgInstantiateContract, MsgExecuteContract
from terra_sdk.core.auth.data.tx import StdFee
from terra_sdk.core.coins import Coins


def store_code(terra_client, wallet, contract_file):
    print("Uploading {0}...".format(contract_file))

    contract_file_bytes = open(contract_file, "rb")
    file_bytes = base64.b64encode(contract_file_bytes.read()).decode()
    store_code_msg = MsgStoreCode(wallet.key.acc_address, file_bytes)
    store_code_tx = wallet.create_and_sign_tx(msgs=[store_code_msg], fee=StdFee(10000000, Coins(uluna=60000)))
    store_code_tx_result = terra_client.tx.broadcast(store_code_tx)
    if store_code_tx_result.code is not None:
        print("Error while storing contract's code: {}".format(store_code_tx_result.raw_log))
        exit(1)

    code_id = store_code_tx_result.logs[0].events_by_type["store_code"]["code_id"][0]

    print("{0} stored with code_id = {1}".format(contract_file, code_id))
    return code_id


def instantiate_contract(terra_client, wallet, code_id, message, coins=None):
    if coins is None:
        coins = {}
    print("Instantiating contract with code_id = {0}...".format(code_id))

    instantiate = MsgInstantiateContract(
        wallet.key.acc_address,
        code_id,
        message,
        coins,
        True,
    )
    instantiate_tx = wallet.create_and_sign_tx(msgs=[instantiate])
    instantiate_tx_result = terra_client.tx.broadcast(instantiate_tx)
    if instantiate_tx_result.code is not None:
        print("Error while instantiating contract: {}".format(instantiate_tx.raw_log))
        exit(1)

    return instantiate_tx_result.logs[0].events_by_type[
        "instantiate_contract"
    ]["contract_address"][0]


def execute_contract(terra_client, wallet, contract_address, message, coins=None):
    if coins is None:
        coins = {}
    execute = MsgExecuteContract(
        wallet.key.acc_address,
        contract_address,
        message,
        coins,
    )

    execute_tx = wallet.create_and_sign_tx(
        msgs=[execute], fee=StdFee(1000000, Coins(uluna=1000000))
    )

    execute_tx_result = terra_client.tx.broadcast(execute_tx)
    if execute_tx_result.code is not None:
        print("Error while executing contract: {}".format(execute_tx_result.raw_log))
        exit(1)

    return execute_tx_result


if __name__ == "__main__":
    terra = LocalTerra()
    test1 = terra.wallets["test1"]

    hub_code_id = store_code(terra, test1, "artifacts/anchor_basset_hub.wasm")
    reward_code_id = store_code(terra, test1, "artifacts/anchor_basset_reward.wasm")
    bluna_token_code_id = store_code(terra, test1, "artifacts/anchor_basset_token.wasm")
    rewards_dispatcher_code_id = store_code(terra, test1, "artifacts/anchor_basset_rewards_dispatcher.wasm")
    validators_registry_code_id = store_code(terra, test1, "artifacts/anchor_basset_validators_registry.wasm")
    stluna_token_code_id = store_code(terra, test1, "artifacts/anchor_basset_token_stluna.wasm")

    print()

    hub_address = instantiate_contract(terra, test1, hub_code_id,
                                       {"epoch_period": 30, "er_threshold": "10000000000000", "peg_recovery_fee": "0",
                                        "reward_denom": "uusd", "unbonding_period": 2, "underlying_coin_denom": "uluna"})

    reward_address = instantiate_contract(terra, test1, reward_code_id,
                                          {"hub_contract": hub_address, "reward_denom": "uusd"})

    bluna_token_address = instantiate_contract(terra, test1, bluna_token_code_id,
                                               {"decimals": 6, "hub_contract": hub_address, "initial_balances": [],
                                                "name": "bluna", "symbol": "BLUNA",
                                                "mint": {"minter": hub_address, "cap": None}})

    rewards_dispatcher_address = instantiate_contract(terra, test1, rewards_dispatcher_code_id,
                                                      {"lido_fee_address": test1.key.acc_address,
                                                       "lido_fee_rate": "0.05", "hub_contract": hub_address, "bluna_reward_contract": reward_address,
                                                       "stluna_reward_denom": "uluna", "bluna_reward_denom": "uusd"})

    validators_registry_address = instantiate_contract(terra, test1, validators_registry_code_id,
                                                       {"hub_contract": hub_address,
                                                        "registry": [{"active": True,
                                                                      "address": "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5",
                                                                      "total_delegated": "0"}]})

    stluna_token_address = instantiate_contract(terra, test1, stluna_token_code_id,
                                                {"decimals": 6, "hub_contract": hub_address, "initial_balances": [],
                                                 "name": "stluna", "symbol": "STLUNA",
                                                 "mint": {"minter": hub_address, "cap": None}})

    print("Updating hub's config...")
    update_hub_config = execute_contract(terra, test1, hub_address, {
        "update_config": {"bluna_token_contract": bluna_token_address, "stluna_token_contract": stluna_token_address,
                          "reward_contract": rewards_dispatcher_address,
                          "validators_registry_contract": validators_registry_address}})
    print()
    print("HUB_CONTRACT = {}".format(hub_address))
    print("REWARD_CONTRACT = {}".format(reward_address))
    print("REWARDS_DISPATCHER_CONTRACT = {}".format(rewards_dispatcher_address))
    print("VALIDATORS_REGISTRY_CONTRACT = {}".format(validators_registry_address))
    print("BLUNA_TOKEN_CONTRACT = {}".format(bluna_token_address))
    print("STLUNA_TOKEN_CONTRACT = {}".format(stluna_token_address))
