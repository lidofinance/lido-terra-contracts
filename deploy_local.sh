echo "Uploading anchor_basset_hub.wasm..."
terracli tx wasm store artifacts/anchor_basset_hub.wasm --from test1 --chain-id=localterra --gas=auto --fees=100000uluna --broadcast-mode=block -y
echo "Done!"

echo "Uploading anchor_basset_reward.wasm..."
terracli tx wasm store artifacts/anchor_basset_reward.wasm --from test1 --chain-id=localterra --gas=auto --fees=100000uluna --broadcast-mode=block -y
echo "Done!"

echo "Uploading anchor_basset_token.wasm..."
terracli tx wasm store artifacts/anchor_basset_token.wasm --from test1 --chain-id=localterra --gas=auto --fees=100000uluna --broadcast-mode=block -y
echo "Done!"

echo "Uploading anchor_basset_rewards_dispatcher.wasm..."
terracli tx wasm store artifacts/anchor_basset_rewards_dispatcher.wasm --from test1 --chain-id=localterra --gas=auto --fees=100000uluna --broadcast-mode=block -y
echo "Done!"

echo "Uploading validators_registry.wasm..."
terracli tx wasm store artifacts/validators_registry.wasm --from test1 --chain-id=localterra --gas=auto --fees=100000uluna --broadcast-mode=block -y
echo "Done!"

echo "Initializing Hub Contract..."
HUB_CONTRACT=$(terracli tx wasm instantiate 1 '{"epoch_period":30,"er_threshold":"10000000000000","peg_recovery_fee":"0","reward_denom":"uusd","unbonding_period":2,"underlying_coin_denom":"uluna"}' --from test1 --chain-id=localterra --fees=10000uluna --gas=auto --broadcast-mode=block --output json -y | jq -r '."logs"[0]."events"[0]."attributes"[2]."value"')
echo "Done!"

echo "Initializing Reward Contract..."
REWARD_CONTRACT=$(terracli tx wasm instantiate 2 "{\"hub_contract\":\"${HUB_CONTRACT}\",\"reward_denom\":\"uusd\"}" --from test1 --chain-id=localterra --fees=10000uluna --gas=auto --broadcast-mode=block --output json -y | jq -r '."logs"[0]."events"[0]."attributes"[2]."value"')
echo "Done!"

echo "Initializing Token Contract..."
TOKEN_CONTRACT=$(terracli tx wasm instantiate 3 "{\"decimals\":6,\"hub_contract\":\"${HUB_CONTRACT}\",\"initial_balances\":[],\"name\":\"bluna\",\"symbol\":\"BLUNA\",\"mint\":{\"minter\":\"${HUB_CONTRACT}\",\"cap\":null}}" --from test1 --chain-id=localterra --fees=10000uluna --gas=auto --broadcast-mode=block --output json -y | jq -r '."logs"[0]."events"[0]."attributes"[2]."value"')
echo "Done!"

echo "Initializing Rewards Dispatcher Contract..."
REWARDS_DISPATCHER_CONTRACT=$(terracli tx wasm instantiate 4 "{\"hub_contract\":\"${HUB_CONTRACT}\",\"stluna_reward_denom\":\"uluna\",\"bluna_reward_denom\":\"uusd\",\"registry\":[{\"active\":true,\"address\":\"terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5\",\"total_delegated\":\"0\"}]}" 100uluna --from test1 --chain-id=localterra --fees=10000uluna --gas=auto --broadcast-mode=block --output json -y | jq -r '."logs"[0]."events"[0]."attributes"[2]."value"')
echo "Done!"

echo "Initializing Validators Registry Contract..."
VR_CONTRACT=$(terracli tx wasm instantiate 5 "{\"hub_contract\":\"${HUB_CONTRACT}\",\"registry\":[{\"active\":true,\"address\":\"terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5\",\"total_delegated\":\"0\"}]}" --from test1 --chain-id=localterra --fees=10000uluna --gas=auto --broadcast-mode=block --output json -y | jq -r '."logs"[0]."events"[0]."attributes"[2]."value"')
echo "Done!"

echo "Updating config with contracts..."
terracli tx wasm execute $HUB_CONTRACT "{\"update_config\":{\"token_contract\":\"${TOKEN_CONTRACT}\",\"reward_contract\":\"${REWARD_CONTRACT}\", \"validators_registry_contract\": \"${VR_CONTRACT}\"}}" --from test1 --chain-id=localterra --fees=1000000uluna --gas=auto --broadcast-mode=block
echo "Done!"

echo "Hub contract address -" $HUB_CONTRACT
echo "Reward contract address -" $REWARD_CONTRACT
echo "Token contract address -" $TOKEN_CONTRACT
echo "Rewards Dispatcher contract address -" $REWARDS_DISPATCHER_CONTRACT
echo "Validators Registry Contract address -" $VR_CONTRACT
