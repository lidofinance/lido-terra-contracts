# 1.0.1

This is a major release that:

1. Modifies the `lido_terra_hub` contract heavily;
2. Introduces the new `lido_terra_token_stluna` token contract;
3. Introduces the new `lido_terra_validators_registry` contract;
4. Introduces the new `lido_terra_rewards_dispatcher` contract;
5. Changes the reward distribution logic and applies a configurable fee to the rewards;
6. Removes the option to pick a validator manually: now a validator is picked from an approved list in a way to make the delegation distribution more even.

See the official [docs](https://lidofinance.github.io/terra-docs/) for details.

#### Modifications to the Hub that may affect third party integrations

1. Hub's `Bond` now doesn't have a `validator` field that allowed the user to pick a specific validator;
2. Hub's `InitMsg` has been modified. Note that now it doesn't have a `validator` field;
3. Hub's `StateResponse` has been modified. Note that instead of `exchange_rate`, we now have `bluna_exchange_rate` and `stluna_exchange_rate`, instead of `total_bond_amount` we have `total_bond_bluna_amount` and `total_bond_stluna_amount`;
4. Hub's `ConfigResponse` has been modified. Note that instead of `token_contract`, we now have `bluna_token_contract` and `stluna_token_contract`;
5. Hub's `whitelisted_validators` query has been removed;
6. Hub's `CurrentBatchResponse` has been modified. Note that instead of `requested_with_fee`, we now have `requested_bluna_with_fee` and `requested_stluna`;
7. Hub's `UnbondRequest` has changed from `Vec<(u64, Uint128)>` `to Vec<(u64, Uint128, Uint128)>` (`<batch_id, bLuna_amount, stLuna_amount>`);
8. Hub's `UnbondHistory` has been modified. Note that instead of `amount` we now have `bluna_amount` and `stluna_amount`, instead of `applied_exchange_rate` we now have `bluna_applied_exchange_rate` and `stluna_applied_exchange_rate`, instead of `withdraw_rate` we now have `bluna_withdraw_rate` and `stluna_withdraw_rate`;


