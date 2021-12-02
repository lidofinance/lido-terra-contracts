# 1.0.1

1. Hub's `Bond` message now doesn't have a `validator` field that allowed the user to pick a specific validator;
2. Hub's `InitMsg` message now doesn't have a `validator` field;
3. Hub's `StateResponse` message has been modified: ```pub struct StateResponse {
   pub bluna_exchange_rate: Decimal,
   pub stluna_exchange_rate: Decimal,
   pub total_bond_bluna_amount: Uint128,
   pub total_bond_stluna_amount: Uint128,
   pub last_index_modification: u64,
   pub prev_hub_balance: Uint128,
   pub last_unbonded_time: u64,
   pub last_processed_batch: u64,
   }```. Instead of `exchange_rate`, we now have `bluna_exchange_rate` and `stluna_exchange_rate`.


