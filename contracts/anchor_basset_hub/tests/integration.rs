// use anchor_basset_hub::contract::{handle, init};
// use anchor_basset_hub::msg::InitMsg;
// use anchor_basset_hub::state::POOL_INFO;
// use anchor_basset_reward::contracts::{
//     handle as reward_handle, init as reward_init, query as reward_query,
// };
// use anchor_basset_reward::msg::HandleMsg::{
//     ClaimRewards, DecreaseBalance, IncreaseBalance, SwapToRewardDenom, UpdateGlobalIndex,
// };
// use anchor_basset_reward::msg::InitMsg as RewardInitMsg;
// use anchor_basset_reward::msg::QueryMsg::{AccruedRewards};
// use anchor_basset_token::contract::{handle as token_handle, init as token_init};
// use cw20_base::msg::HandleMsg::{Burn, Mint, Send, Transfer};
// use anchor_basset_token::msg::TokenInitMsg;
// use cosmwasm_std::testing::{mock_env, MOCK_CONTRACT_ADDR};
// use cosmwasm_std::{
//     coin, from_binary, to_binary, Api, BankMsg, CanonicalAddr, CosmosMsg, Decimal, Extern,
//     HumanAddr, Querier, StakingMsg, StdError, StdResult, Storage, Uint128, Validator, WasmMsg,
// };
// use cosmwasm_storage::Singleton;
// use cw20::{Cw20ReceiveMsg, MinterResponse};
// use gov_courier::Registration::{Reward, Token};
// use gov_courier::{Cw20HookMsg, HandleMsg, PoolInfo};

// mod common;
// use anchor_basset_reward::msg::{AccruedRewardsResponse, IndexResponse, PendingRewardsResponse};
// use common::mock_querier::{mock_dependencies as dependencies, WasmMockQuerier};
// use gov_courier::HandleMsg::UpdateParams;
// use terra_cosmwasm::create_swap_msg;

// const TOKEN_INFO_KEY: &[u8] = b"token_info";
// const DEFAULT_VALIDATOR: &str = "default-validator";
// const DEFAULT_VALIDATOR2: &str = "default-validator2";
// pub static CONFIG: &[u8] = b"config";

// pub fn init_all<S: Storage, A: Api, Q: Querier>(
//     mut deps: &mut Extern<S, A, Q>,
//     owner: HumanAddr,
//     reward_contract: HumanAddr,
//     token_contract: HumanAddr,
// ) {
//     let msg = InitMsg {
//         epoch_time: 30,
//         underlying_coin_denom: "uluna".to_string(),
//         undelegated_epoch: 2,
//         peg_recovery_fee: Decimal::zero(),
//         er_threshold: Decimal::one(),
//         reward_denom: "uusd".to_string(),
//     };

//     let env = mock_env(owner.clone(), &[]);
//     let res = init(&mut deps, env, msg).unwrap();
//     assert_eq!(res.messages.len(), 2);

//     let gov_address = deps
//         .api
//         .canonical_address(&HumanAddr::from(MOCK_CONTRACT_ADDR))
//         .unwrap();

//     let gov_env = mock_env(HumanAddr::from(MOCK_CONTRACT_ADDR), &[]);
//     let reward_in = default_reward(HumanAddr::from(MOCK_CONTRACT_ADDR), "uusd".to_string());
//     reward_init(&mut deps, gov_env.clone(), reward_in).unwrap();

//     let token_int = default_token(HumanAddr::from(MOCK_CONTRACT_ADDR), owner);
//     token_init(&mut deps, gov_env, token_int).unwrap();

//     let register_msg = HandleMsg::RegisterSubcontracts { contract: Reward };
//     let register_env = mock_env(reward_contract, &[]);
//     handle(&mut deps, register_env, register_msg).unwrap();

//     let register_msg = HandleMsg::RegisterSubcontracts { contract: Token };
//     let register_env = mock_env(token_contract, &[]);
//     handle(&mut deps, register_env, register_msg).unwrap();
// }

// pub fn do_bond<S: Storage, A: Api, Q: Querier>(
//     mut deps: &mut Extern<S, A, Q>,
//     addr: HumanAddr,
//     amount: Uint128,
//     validator: Validator,
// ) {
//     let owner = HumanAddr::from("owner1");

//     let owner_env = mock_env(owner, &[]);
//     let msg = HandleMsg::RegisterValidator {
//         validator: validator.address.clone(),
//     };

//     let res = handle(&mut deps, owner_env, msg).unwrap();
//     assert_eq!(0, res.messages.len());

//     let bond = HandleMsg::Bond {
//         validator: validator.address,
//     };

//     let env = mock_env(&addr, &[coin(amount.0, "uluna")]);
//     let _res = handle(&mut deps, env, bond);
//     let msg = Mint {
//         recipient: addr.clone(),
//         amount,
//     };

//     let owner = HumanAddr::from(MOCK_CONTRACT_ADDR);
//     let env = mock_env(&owner, &[]);
//     let res = token_handle(&mut deps, env, msg).unwrap();
//     assert_eq!(1, res.messages.len());
//     assert_eq!(
//         res.messages,
//         vec![CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: HumanAddr::from("reward"),
//             msg: to_binary(&IncreaseBalance {
//                 address: addr.clone(),
//                 amount,
//             })
//             .unwrap(),
//             send: vec![],
//         })]
//     );
// }

// pub fn do_update_user_in<S: Storage, A: Api, Q: Querier>(
//     mut deps: &mut Extern<S, A, Q>,
//     address: HumanAddr,
//     amount: Uint128,
// ) {
//     let update_user_index = IncreaseBalance {
//         address,
//         amount,
//     };

//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);
// }

// pub fn do_update_global<S: Storage, A: Api, Q: Querier>(
//     deps: &mut Extern<S, A, Q>,
//     expected_res: &str,
// ) {
//     let reward_msg = HandleMsg::UpdateGlobalIndex {};

//     let env = mock_env(&HumanAddr::from("owner1"), &[]);
//     let res = handle(deps, env, reward_msg).unwrap();
//     assert_eq!(3, res.messages.len());

//     reward_update_global(deps, expected_res);
// }

// pub fn reward_update_global<S: Storage, A: Api, Q: Querier>(
//     deps: &mut Extern<S, A, Q>,
//     expected_res: &str,
// ) {
//     let owner = HumanAddr::from(MOCK_CONTRACT_ADDR);
//     let mut env = mock_env(&owner, &[]);
//     env.contract.address = HumanAddr::from("reward");

//     let update_global_index = UpdateGlobalIndex {
//         prev_balance: Uint128::zero(),
//     };
//     let reward_update = reward_handle(deps, env, update_global_index).unwrap();
//     assert_eq!(reward_update.messages.len(), 0);

//     //check the expected index
//     let query = GlobalIndex {};
//     let qry = reward_query(&deps, query).unwrap();
//     let res: IndexResponse = from_binary(&qry).unwrap();
//     assert_eq!(res.index.to_string(), expected_res);
// }

// fn sample_validator<U: Into<HumanAddr>>(addr: U) -> Validator {
//     Validator {
//         address: addr.into(),
//         commission: Decimal::percent(3),
//         max_commission: Decimal::percent(10),
//         max_change_rate: Decimal::percent(1),
//     }
// }

// fn set_validator_mock(querier: &mut WasmMockQuerier) {
//     querier.update_staking(
//         "uluna",
//         &[
//             sample_validator(DEFAULT_VALIDATOR),
//             sample_validator(DEFAULT_VALIDATOR2),
//         ],
//         &[],
//     );
// }

// fn default_reward(hub_contract: HumanAddr, reward_denom: String) -> RewardInitMsg {
//     RewardInitMsg {
//         hub_contract,
//         reward_denom,
//     }
// }

// pub fn set_pool_info<S: Storage>(
//     storage: &mut S,
//     ex_rate: Decimal,
//     total_boned: Uint128,
//     reward_account: CanonicalAddr,
//     token_account: CanonicalAddr,
// ) -> StdResult<()> {
//     Singleton::new(storage, POOL_INFO).save(&PoolInfo {
//         exchange_rate: ex_rate,
//         total_bond_amount: total_boned,
//         last_index_modification: 0,
//         reward_account,
//         is_reward_exist: true,
//         is_token_exist: true,
//         token_account,
//     })
// }

// fn default_token(hub_contract: HumanAddr, minter: HumanAddr) -> TokenInitMsg {
//     TokenInitMsg {
//         name: "bluna".to_string(),
//         symbol: "BLUNA".to_string(),
//         decimals: 6,
//         initial_balances: vec![],
//         mint: Some(MinterResponse { minter, cap: None }),
//         hub_contract,
//     }
// }

// //this will check the update global index workflow
// #[test]
// fn integrated_update_global_index() {
//     let mut deps = dependencies(20, &[]);
//     let validator = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     let invalid_sender = HumanAddr::from("invalid");

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     init_all(
//         &mut deps,
//         owner.clone(),
//         reward_contract.clone(),
//         token_contract,
//     );

//     let env = mock_env(&owner, &[]);
//     let msg = HandleMsg::RegisterValidator {
//         validator: validator.address.clone(),
//     };

//     let res = handle(&mut deps, env, msg).unwrap();
//     assert_eq!(0, res.messages.len());

//     let bob = HumanAddr::from("bob");
//     let bond_msg = HandleMsg::Bond {
//         validator: validator.address.clone(),
//     };

//     let env = mock_env(&bob, &[coin(10, "uluna")]);

//     //set bob's balance to 10 in token contract
//     deps.querier
//         .with_token_balances(&[(&HumanAddr::from("token"), &[(&bob, &Uint128(10u128))])]);

//     let res = handle(&mut deps, env, bond_msg).unwrap();
//     assert_eq!(2, res.messages.len());

//     let token_mint = Mint {
//         recipient: bob.clone(),
//         amount: Uint128(10),
//     };
//     let gov_env = mock_env(MOCK_CONTRACT_ADDR, &[]);
//     let token_res = token_handle(&mut deps, gov_env, token_mint).unwrap();
//     assert_eq!(1, token_res.messages.len());

//     let reward_msg = HandleMsg::UpdateGlobalIndex {};

//     let env = mock_env(&bob, &[]);
//     let res = handle(&mut deps, env, reward_msg).unwrap();
//     assert_eq!(3, res.messages.len());

//     let withdraw = &res.messages[0];
//     match withdraw {
//         CosmosMsg::Staking(StakingMsg::Withdraw {
//             validator: val,
//             recipient,
//         }) => {
//             assert_eq!(val, &validator.address);
//             assert_eq!(recipient.is_none(), true);
//         }
//         _ => panic!("Unexpected message: {:?}", withdraw),
//     }

//     let swap = &res.messages[1];
//     match swap {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(contract_addr, &reward_contract);
//             assert_eq!(msg, &to_binary(&SwapToRewardDenom {}).unwrap())
//         }
//         _ => panic!("Unexpected message: {:?}", swap),
//     }

//     let update_g_index = &res.messages[2];
//     match update_g_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(contract_addr, &reward_contract);
//             assert_eq!(msg, &to_binary(&UpdateGlobalIndex {prev_balance: Uint128::zero()}).unwrap())
//         }
//         _ => panic!("Unexpected message: {:?}", update_g_index),
//     }

//     //send update global index and check the expected reward
//     reward_update_global(&mut deps, "200");

//     //read the previous balance from the storage and check the amount
//     let previous_balance = prev_balance_read(&deps.storage).load().unwrap();
//     assert_eq!(previous_balance, Uint128(2000));

//     // sender is not gov contract
//     let update_global_index = UpdateGlobalIndex {prev_balance: Uint128::zero()};
//     let invalid_env = mock_env(&invalid_sender, &[]);
//     let reward_update = reward_handle(&mut deps, invalid_env, update_global_index);
//     assert_eq!(reward_update.unwrap_err(), StdError::unauthorized());
// }

// #[test]
// pub fn proper_update_user_index() {
//     let mut deps = dependencies(20, &[]);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let val = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     init_all(&mut deps, owner, reward_contract, token_contract);
//     let addr1 = HumanAddr::from("addr0001");

//     //first bond
//     do_bond(&mut deps, addr1.clone(), Uint128(10), val.clone());
//     let update_user_index = IncreaseBalance {
//         address: addr1.clone(),
//         amount: Uint128(10),
//     };
//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);

//     let query_index = UserIndex {
//         address: addr1.clone(),
//     };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "0");

//     //set bob's balance to 10 in token contract
//     deps.querier
//         .with_token_balances(&[(&HumanAddr::from("token"), &[(&addr1, &Uint128(10u128))])]);

//     //update global_index
//     do_update_global(&mut deps, "200");

//     //second bond
//     do_bond(&mut deps, addr1.clone(), Uint128(10), val);

//     //send unauthorized update user index
//     let update_user_index = IncreaseBalance {
//         address: addr1.clone(),
//         amount: Uint128(10),
//     };
//     let invalid = HumanAddr::from("invalid");
//     let invalid_env = mock_env(invalid, &[]);
//     let res = reward_handle(&mut deps, invalid_env, update_user_index.clone());
//     assert_eq!(res.unwrap_err(), StdError::unauthorized());

//     //send update user index
//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);

//     //get the index of the user
//     let query_index = UserIndex {
//         address: addr1.clone(),
//     };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "200");

//     //get the pending reward of the user
//     let query_pending = PendingRewards { address: addr1 };
//     let query_res = reward_query(&deps, query_pending).unwrap();
//     let pending: PendingRewardsResponse = from_binary(&query_res).unwrap();
//     assert_eq!(pending.rewards, Uint128(2000));
// }

// #[test]
// pub fn integrated_claim_rewards() {
//     let mut deps = dependencies(20, &[]);

//     //add tax
//     deps.querier._with_tax(
//         Decimal::percent(1),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );

//     let addr1 = HumanAddr::from("addr0001");
//     let addr2 = HumanAddr::from("addr0002");
//     let amount1 = Uint128::from(10u128);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let val = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     init_all(&mut deps, owner, reward_contract, token_contract);

//     //failed ClaimRewards since there is no user yet
//     let failed_calim = ClaimRewards { recipient: None };

//     let mut env = mock_env(&addr1, &[]);
//     env.contract.address = HumanAddr::from("reward");
//     let res = reward_handle(&mut deps, env, failed_calim);
//     assert_eq!(
//         res.unwrap_err(),
//         StdError::generic_err("The sender has not requested any tokens")
//     );

//     //bond first
//     do_bond(&mut deps, addr1.clone(), amount1, val.clone());
//     do_bond(&mut deps, addr2.clone(), amount1, val);

//     //update user index
//     do_update_user_in(&mut deps, addr1.clone(), amount1);
//     do_update_user_in(&mut deps, addr2.clone(), amount1);

//     //failed ClaimRewards since there is no reward yet
//     let failed_calim = ClaimRewards { recipient: None };

//     let mut env = mock_env(&addr1, &[]);
//     env.contract.address = HumanAddr::from("reward");
//     let res = reward_handle(&mut deps, env, failed_calim);
//     assert_eq!(
//         res.unwrap_err(),
//         StdError::generic_err("There is no reward yet for the user")
//     );

//     //set addr1's balance to 10 in token contract
//     deps.querier.with_token_balances(&[(
//         &HumanAddr::from("token"),
//         &[(&addr1, &Uint128(10u128)), (&addr2, &Uint128(10u128))],
//     )]);

//     //update global_index
//     do_update_global(&mut deps, "100");

//     //send ClaimRewards by the sender
//     let claim = ClaimRewards { recipient: None };

//     let mut env = mock_env(&addr1, &[]);
//     env.contract.address = HumanAddr::from("reward");
//     let res = reward_handle(&mut deps, env, claim).unwrap();
//     assert_eq!(res.messages.len(), 1);

//     let send = &res.messages[0];
//     // since the global index is 100 and the user balance is 10.
//     // user's rewards is 10 * 100 = 1000, but the user should receive 990 as a reward.
//     // 10 uusd is deducted because of tax.
//     match send {
//         CosmosMsg::Bank(BankMsg::Send {
//             from_address,
//             to_address,
//             amount,
//         }) => {
//             assert_eq!(from_address, &HumanAddr::from("reward"));
//             assert_eq!(to_address, &addr1);
//             //the tax is 1 percent there fore 1000 - 10 = 990
//             assert_eq!(amount.get(0).unwrap().amount, Uint128(990));
//         }
//         _ => panic!("Unexpected message: {:?}", send),
//     }

//     //get the index of the user
//     let query_index = UserIndex {
//         address: addr1.clone(),
//     };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "100");

//     //get the pending reward of the user
//     let query_index = PendingRewards { address: addr1 };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let pending: PendingRewardsResponse = from_binary(&query_res).unwrap();
//     assert_eq!(pending.rewards, Uint128(0));
// }

// #[test]
// pub fn integrated_swap() {
//     let mut deps = dependencies(20, &[]);
//     let gov = HumanAddr::from(MOCK_CONTRACT_ADDR);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let mut env = mock_env(&gov, &[]);
//     env.contract.address = HumanAddr::from("reward");

//     init_all(&mut deps, owner, reward_contract.clone(), token_contract);

//     let swap = SwapToRewardDenom {};
//     let reward_update = reward_handle(&mut deps, env, swap).unwrap();
//     // there are three coins including uusd, therefore we will have 2 swap messages.
//     // uusd should not be swapped.
//     assert_eq!(reward_update.messages.len(), 2);

//     let msg_luna = create_swap_msg(
//         reward_contract.clone(),
//         coin(1000, "uluna"),
//         "uusd".to_string(),
//     );
//     assert_eq!(reward_update.messages[0], msg_luna);
//     let msg_krt = create_swap_msg(reward_contract, coin(1000, "ukrt"), "uusd".to_string());
//     assert_eq!(reward_update.messages[1], msg_krt);
// }

// #[test]
// pub fn integrated_transfer() {
//     let mut deps = dependencies(20, &[]);

//     //add tax
//     deps.querier._with_tax(
//         Decimal::percent(1),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );

//     let addr1 = HumanAddr::from("addr0001");
//     let addr2 = HumanAddr::from("addr0002");
//     let addr3 = HumanAddr::from("addr0003");
//     let addr4 = HumanAddr::from("addr0004");

//     let amount1 = Uint128::from(10u128);
//     let amount2 = Uint128(2).multiply_ratio(amount1, Uint128(1));
//     let transfer = Uint128::from(1u128);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let val = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     init_all(&mut deps, owner, reward_contract, token_contract);

//     //bond first
//     do_bond(&mut deps, addr1.clone(), amount1, val.clone());
//     do_bond(&mut deps, addr2.clone(), amount1, val.clone());
//     do_bond(&mut deps, addr4.clone(), amount2, val);

//     //update user index
//     do_update_user_in(&mut deps, addr1.clone(), amount1);
//     do_update_user_in(&mut deps, addr2.clone(), amount1);
//     do_update_user_in(&mut deps, addr4.clone(), amount2);

//     //set addr1's balance to 10 in token contract
//     deps.querier.with_token_balances(&[(
//         &HumanAddr::from("token"),
//         &[
//             (&addr1, &Uint128(10u128)),
//             (&addr2, &Uint128(10u128)),
//             (&addr4, &Uint128(20u128)),
//         ],
//     )]);

//     //update global_index
//     do_update_global(&mut deps, "50");

//     let env = mock_env(addr1.clone(), &[]);
//     let msg = Transfer {
//         recipient: addr2.clone(),
//         amount: transfer,
//     };
//     let res = token_handle(&mut deps, env, msg).unwrap();
//     assert_eq!(res.messages.len(), 2);

//     let update_addr1_index = &res.messages[0];
//     match update_addr1_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&DecreaseBalance {
//                     address: addr1.clone(),
//                     amount: transfer
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }

//     let update_addr2_index = &res.messages[1];
//     match update_addr2_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&IncreaseBalance {
//                     address: addr2.clone(),
//                     amount: transfer
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }

//     let claim = ClaimRewards {
//         recipient: Some(addr1.clone()),
//     };

//     let mut env = mock_env(HumanAddr::from("token"), &[]);
//     env.contract.address = HumanAddr::from("reward");
//     let res = reward_handle(&mut deps, env, claim).unwrap();
//     assert_eq!(res.messages.len(), 1);

//     let send = &res.messages[0];
//     match send {
//         CosmosMsg::Bank(BankMsg::Send {
//             from_address,
//             to_address,
//             amount,
//         }) => {
//             assert_eq!(from_address, &HumanAddr::from("reward"));
//             assert_eq!(to_address, &addr1);
//             //the tax is 1 percent there fore 500 - 5 = 490
//             assert_eq!(amount.get(0).unwrap().amount, Uint128(495));
//         }
//         _ => panic!("Unexpected message: {:?}", send),
//     }

//     //get the index of the user
//     let query_index = UserIndex { address: addr1 };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "50");

//     //send update user index
//     let update_user_index = IncreaseBalance {
//         address: addr2.clone(),
//         amount: transfer,
//     };
//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);

//     //get the index of the user
//     let query_index = UserIndex {
//         address: addr2.clone(),
//     };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "50");

//     //get the pending reward of the user
//     let query_pending = PendingRewards { address: addr2 };
//     let query_res = reward_query(&deps, query_pending).unwrap();
//     let pending: PendingRewardsResponse = from_binary(&query_res).unwrap();
//     assert_eq!(pending.rewards, Uint128(500));

//     let env = mock_env(addr4, &[]);
//     let msg = Transfer {
//         recipient: addr3.clone(),
//         amount: transfer,
//     };
//     let res = token_handle(&mut deps, env, msg).unwrap();
//     assert_eq!(res.messages.len(), 2);

//     let update_addr2_index = &res.messages[1];
//     match update_addr2_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&IncreaseBalance {
//                     address: addr3.clone(),
//                     amount: transfer
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }
//     //get the index of user should be first error
//     let query_index = UserIndex {
//         address: addr3.clone(),
//     };
//     let query_res = reward_query(&deps, query_index);
//     assert_eq!(
//         query_res.unwrap_err(),
//         StdError::generic_err("no holder is found")
//     );

//     //send update user index
//     let update_user_index = IncreaseBalance {
//         address: addr3.clone(),
//         amount: transfer,
//     };
//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);

//     //get the index of the user
//     let query_index = UserIndex { address: addr3 };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "50");
// }

// #[test]
// pub fn integrated_send() {
//     let mut deps = dependencies(20, &[]);

//     //add tax
//     deps.querier._with_tax(
//         Decimal::percent(1),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );

//     let addr1 = HumanAddr::from("addr0001");
//     let contract = HumanAddr::from(MOCK_CONTRACT_ADDR);
//     let amount1 = Uint128::from(10u128);
//     let transfer = Uint128::from(1u128);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let val = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     init_all(&mut deps, owner, reward_contract, token_contract);

//     //bond first
//     do_bond(&mut deps, addr1.clone(), amount1, val.clone());
//     do_bond(&mut deps, contract.clone(), amount1, val);

//     //update user index
//     do_update_user_in(&mut deps, addr1.clone(), amount1);
//     do_update_user_in(&mut deps, contract.clone(), amount1);

//     //set addr1's balance to 10 in token contract
//     deps.querier.with_token_balances(&[(
//         &HumanAddr::from("token"),
//         &[(&addr1, &Uint128(10u128)), (&contract, &Uint128(10u128))],
//     )]);

//     //update global_index
//     do_update_global(&mut deps, "100");

//     let env = mock_env(addr1.clone(), &[]);
//     let send_msg = Send {
//         contract: contract.clone(),
//         amount: transfer,
//         msg: Some(to_binary(&Cw20HookMsg::Unbond {}).unwrap()),
//     };
//     let res = token_handle(&mut deps, env, send_msg).unwrap();
//     assert_eq!(res.messages.len(), 3);

//     let update_addr1_index = &res.messages[0];
//     match update_addr1_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&DecreaseBalance {
//                     address: addr1.clone(),
//                     amount: transfer,
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }

//     let update_addr1_index = &res.messages[1];
//     match update_addr1_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&IncreaseBalance {
//                     address: contract.clone(),
//                     amount: transfer
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }

//     let send_msg = to_binary(&Cw20HookMsg::Unbond {}).unwrap();

//     let binary_msg = Cw20ReceiveMsg {
//         sender: addr1,
//         amount: transfer,
//         msg: Some(send_msg),
//     }
//     .into_binary()
//     .unwrap();

//     assert_eq!(
//         res.messages[2],
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: contract.clone(),
//             msg: binary_msg,
//             send: vec![],
//         })
//     );

//     //send update user index
//     let update_user_index = DecreaseBalance {
//         address: contract.clone(),
//         amount: transfer,
//     };
//     let token = HumanAddr::from("token");
//     let token_env = mock_env(token, &[]);
//     let res = reward_handle(&mut deps, token_env, update_user_index).unwrap();
//     assert_eq!(res.messages.len(), 0);

//     //get the index of the user
//     let query_index = UserIndex {
//         address: contract.clone(),
//     };
//     let query_res = reward_query(&deps, query_index).unwrap();
//     let index: IndexResponse = from_binary(&query_res).unwrap();
//     assert_eq!(index.index.to_string(), "100");

//     //get the pending reward of the user
//     let query_pending = PendingRewards {
//         address: contract.clone(),
//     };
//     let query_res = reward_query(&deps, query_pending).unwrap();
//     let pending: PendingRewardsResponse = from_binary(&query_res).unwrap();
//     assert_eq!(pending.rewards, Uint128(1000));

//     //get the accrued rewards
//     let query_accrued = AccruedRewards { address: contract };
//     let query_res = reward_query(&deps, query_accrued).unwrap();
//     let pending: AccruedRewardsResponse = from_binary(&query_res).unwrap();
//     assert_eq!(pending.rewards, Uint128(1000));
// }

// #[test]
// pub fn integrated_burn() {
//     let mut deps = dependencies(20, &[]);

//     //add tax
//     deps.querier._with_tax(
//         Decimal::percent(1),
//         &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
//     );

//     let contract = HumanAddr::from(MOCK_CONTRACT_ADDR);
//     let amount1 = Uint128::from(10u128);

//     let owner = HumanAddr::from("owner1");
//     let token_contract = HumanAddr::from("token");
//     let reward_contract = HumanAddr::from("reward");

//     let val = sample_validator(DEFAULT_VALIDATOR);
//     set_validator_mock(&mut deps.querier);

//     init_all(&mut deps, owner, reward_contract, token_contract);

//     //bond first
//     do_bond(&mut deps, contract.clone(), amount1, val);

//     //update user index
//     do_update_user_in(&mut deps, contract.clone(), amount1);

//     //set addr1's balance to 10 in token contract
//     deps.querier
//         .with_token_balances(&[(&HumanAddr::from("token"), &[(&contract, &Uint128(10u128))])]);

//     //update global_index
//     do_update_global(&mut deps, "200");

//     let env = mock_env(contract.clone(), &[]);
//     let burn = Burn { amount: amount1 };
//     let res = token_handle(&mut deps, env, burn).unwrap();
//     assert_eq!(res.messages.len(), 1);

//     let update_contract_index = &res.messages[0];
//     match update_contract_index {
//         CosmosMsg::Wasm(WasmMsg::Execute {
//             contract_addr: _,
//             msg,
//             send: _,
//         }) => {
//             assert_eq!(
//                 msg,
//                 &to_binary(&DecreaseBalance {
//                     address: contract,
//                     amount: Uint128(10)
//                 })
//                 .unwrap()
//             );
//         }
//         _ => panic!("Unexpected message: {:?}",),
//     }
// }
