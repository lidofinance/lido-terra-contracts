use crate::common::{calculate_delegations, calculate_undelegations};
use crate::contract::{handle, init};
use crate::msg::{HandleMsg, InitMsg};
use crate::registry::{config_read, registry_read, Validator};
use crate::testing::mock_querier::{mock_dependencies, WasmMockQuerier};
use cosmwasm_std::testing::mock_env;
use cosmwasm_std::{
    coin, coins, to_binary, Api, Coin, CosmosMsg, FullDelegation, HumanAddr, StdError, Uint128,
    Validator as CosmosValidator, WasmMsg,
};
use hub_querier::HandleMsg::{RedelegateProxy, UpdateGlobalIndex};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(20, &[]);

    let hub_address = HumanAddr::from("hub_contract_address");

    let msg = InitMsg {
        registry: vec![Validator {
            total_delegated: Default::default(),
            address: Default::default(),
        }],
        hub_contract: hub_address.clone(),
    };
    let env = mock_env("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = init(&mut deps, env, msg).unwrap();
    assert_eq!(0, res.messages.len());

    assert_eq!(
        config_read(&deps.storage).load().unwrap().hub_contract,
        deps.api.canonical_address(&hub_address).unwrap()
    )
}

#[test]
fn add_validator() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));

    let msg = InitMsg {
        registry: vec![],
        hub_contract: HumanAddr::from("hub_contract_address"),
    };
    let env = mock_env("creator", &coins(2, "token"));
    let _res = init(&mut deps, env.clone(), msg).unwrap();

    let validator = Validator {
        total_delegated: Default::default(),
        address: Default::default(),
    };

    let msg = HandleMsg::AddValidator {
        validator: validator.clone(),
    };
    let _res = handle(&mut deps, env, msg);

    match _res {
        Ok(_) => {
            let v = registry_read(&deps.storage)
                .load(validator.address.as_str().as_bytes())
                .unwrap();
            assert_eq!(validator, v);
        }
        Err(e) => panic!(format!("Failed to handle AddValidator message: {}", e)),
    }
}

#[test]
fn ownership_tests() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));

    let msg = InitMsg {
        registry: vec![],
        hub_contract: HumanAddr::from("hub_contract_address"),
    };
    let env = mock_env("creator", &coins(2, "token"));
    let _res = init(&mut deps, env, msg).unwrap();

    let env = mock_env("villain", &coins(2, "token"));

    let validator = Validator {
        total_delegated: Default::default(),
        address: Default::default(),
    };

    let msg = HandleMsg::AddValidator {
        validator: validator.clone(),
    };
    let res = handle(&mut deps, env.clone(), msg);
    assert_eq!(res.err().unwrap(), StdError::unauthorized());

    let msg = HandleMsg::RemoveValidator {
        address: validator.address,
    };
    let res = handle(&mut deps, env.clone(), msg);
    assert_eq!(res.err().unwrap(), StdError::unauthorized());

    let msg = HandleMsg::UpdateConfig {
        hub_contract: None,
        owner: None,
    };
    let res = handle(&mut deps, env, msg);
    assert_eq!(res.err().unwrap(), StdError::unauthorized());
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));

    let msg = InitMsg {
        registry: vec![],
        hub_contract: HumanAddr::from("hub_contract_address"),
    };
    let env = mock_env("creator", &coins(2, "token"));
    let _res = init(&mut deps, env.clone(), msg).unwrap();

    let new_hub_address = HumanAddr::from("new_hub_contract");
    let msg = HandleMsg::UpdateConfig {
        hub_contract: Some(new_hub_address.clone()),
        owner: None,
    };
    let res = handle(&mut deps, env.clone(), msg);
    assert!(res.is_ok());
    let config = config_read(&deps.storage).load().unwrap();
    assert_eq!(
        deps.api.canonical_address(&new_hub_address).unwrap(),
        config.hub_contract
    );

    let new_owner = HumanAddr::from("new_owner");
    let msg = HandleMsg::UpdateConfig {
        owner: Some(new_owner.clone()),
        hub_contract: None,
    };
    let res = handle(&mut deps, env, msg);
    assert!(res.is_ok());
    let config = config_read(&deps.storage).load().unwrap();
    assert_eq!(
        deps.api.canonical_address(&new_owner).unwrap(),
        config.owner
    );
}

#[test]
fn remove_validator() {
    let mut deps = mock_dependencies(20, &coins(2, "token"));
    let hub_contract_address = HumanAddr::from("hub_contract_address");
    let validator1 = Validator {
        total_delegated: Uint128(0),
        address: HumanAddr::from("validator"),
    };

    let validator2 = Validator {
        total_delegated: Uint128(0),
        address: HumanAddr::from("validator2"),
    };

    let validator3 = Validator {
        total_delegated: Uint128(0),
        address: HumanAddr::from("validator3"),
    };

    let validator4 = Validator {
        total_delegated: Uint128(0),
        address: HumanAddr::from("validator4"),
    };

    let validators = [
        CosmosValidator {
            address: validator1.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator2.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator3.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator4.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
    ];
    set_delegation_query(
        &mut deps.querier,
        &[
            sample_delegation(
                hub_contract_address.clone(),
                validator1.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(10u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator2.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(20u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator3.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(30u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator4.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(50u64),
                },
            ),
        ],
        &validators,
    );
    let msg = InitMsg {
        registry: vec![
            validator1.clone(),
            validator2.clone(),
            validator3.clone(),
            validator4.clone(),
        ],
        hub_contract: hub_contract_address.clone(),
    };

    let env = mock_env("creator", &coins(2, "token"));
    let _res = init(&mut deps, env.clone(), msg).unwrap();

    // try to remove validator4
    let msg = HandleMsg::RemoveValidator {
        address: validator4.address.clone(),
    };
    let _res = handle(&mut deps, env.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = registry_read(&deps.storage).load(validator4.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator4.address.clone(),
                            dst_validator: validator1.clone().address,
                            amount: coin(27, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let redelegate = &res.messages[1];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator4.address.clone(),
                            dst_validator: validator2.clone().address,
                            amount: coin(17, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let redelegate = &res.messages[2];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator4.address,
                            dst_validator: validator3.clone().address,
                            amount: coin(6, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[3];
            match update_global_index {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                    );
                    assert_eq!(*contract_addr, HumanAddr::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!(format!("Failed to handle RemoveValidator message: {}", e)),
    }

    // try to remove validator 3
    let validators = [
        CosmosValidator {
            address: validator1.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator2.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator3.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
    ];
    set_delegation_query(
        &mut deps.querier,
        &[
            sample_delegation(
                hub_contract_address.clone(),
                validator1.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(37u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator2.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(37u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator3.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(36u64),
                },
            ),
        ],
        &validators,
    );
    let msg = HandleMsg::RemoveValidator {
        address: validator3.address.clone(),
    };
    let _res = handle(&mut deps, env.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = registry_read(&deps.storage).load(validator3.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator3.address.clone(),
                            dst_validator: validator1.clone().address,
                            amount: coin(18, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let redelegate = &res.messages[1];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator3.address,
                            dst_validator: validator2.clone().address,
                            amount: coin(18, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[2];
            match update_global_index {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                    );
                    assert_eq!(*contract_addr, HumanAddr::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!(format!("Failed to handle RemoveValidator message: {}", e)),
    }

    // try to remove validator 2
    let validators = [
        CosmosValidator {
            address: validator1.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
        CosmosValidator {
            address: validator2.address.clone(),
            commission: Default::default(),
            max_commission: Default::default(),
            max_change_rate: Default::default(),
        },
    ];
    set_delegation_query(
        &mut deps.querier,
        &[
            sample_delegation(
                hub_contract_address.clone(),
                validator1.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(55u64),
                },
            ),
            sample_delegation(
                hub_contract_address.clone(),
                validator2.address.clone(),
                Coin {
                    denom: "uluna".to_string(),
                    amount: Uint128::from(55u64),
                },
            ),
        ],
        &validators,
    );
    let msg = HandleMsg::RemoveValidator {
        address: validator2.address.clone(),
    };
    let _res = handle(&mut deps, env.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = registry_read(&deps.storage).load(validator2.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&RedelegateProxy {
                            src_validator: validator2.address,
                            dst_validator: validator1.clone().address,
                            amount: coin(55, "uluna"),
                        })
                        .unwrap()
                    );
                    assert_eq!(contract_addr.to_string(), hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[1];
            match update_global_index {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    send: _,
                }) => {
                    assert_eq!(
                        *msg,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                    );
                    assert_eq!(*contract_addr, HumanAddr::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!(format!("Failed to handle RemoveValidator message: {}", e)),
    }

    // try to remove the last validator
    let validators = [CosmosValidator {
        address: validator1.address.clone(),
        commission: Default::default(),
        max_commission: Default::default(),
        max_change_rate: Default::default(),
    }];
    set_delegation_query(
        &mut deps.querier,
        &[sample_delegation(
            hub_contract_address,
            validator1.address.clone(),
            Coin {
                denom: "uluna".to_string(),
                amount: Uint128::from(110u64),
            },
        )],
        &validators,
    );
    let msg = HandleMsg::RemoveValidator {
        address: validator1.address,
    };
    let res = handle(&mut deps, env, msg);
    assert_eq!(
        res.expect_err("The last validator was removed from registry"),
        StdError::generic_err("Cannot remove the last validator in the registry",)
    );
}

#[macro_export]
macro_rules! default_validator_with_delegations {
    ($total:expr) => {
        Validator {
            total_delegated: Uint128($total),
            address: Default::default(),
        }
    };
}

//TODO: implement more test cases
#[test]
fn test_calculate_delegations() {
    let mut validators = vec![
        default_validator_with_delegations!(0),
        default_validator_with_delegations!(0),
        default_validator_with_delegations!(0),
    ];
    let expected_delegations: Vec<Uint128> = vec![Uint128(4), Uint128(3), Uint128(3)];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

    let buffered_balance = Uint128(10);
    let (remained_balance, delegations) =
        calculate_delegations(buffered_balance, validators.as_slice()).unwrap();

    assert_eq!(
        validators.len(),
        delegations.len(),
        "Delegations are not correct"
    );
    assert_eq!(
        remained_balance,
        Uint128(0),
        "Not all tokens were delegated"
    );
    for i in 0..expected_delegations.len() {
        assert_eq!(
            delegations[i], expected_delegations[i],
            "Delegation is not correct"
        )
    }
}

//TODO: implement more test cases
#[test]
fn test_calculate_undelegations() {
    let mut validators = vec![
        default_validator_with_delegations!(100),
        default_validator_with_delegations!(10),
        default_validator_with_delegations!(10),
    ];
    let expected_undelegations: Vec<Uint128> = vec![Uint128(93), Uint128(3), Uint128(4)];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

    let undelegate_amount = Uint128(100);
    let undelegations = calculate_undelegations(undelegate_amount, validators.as_slice()).unwrap();

    assert_eq!(
        validators.len(),
        undelegations.len(),
        "Delegations are not correct"
    );
    for i in 0..expected_undelegations.len() {
        assert_eq!(
            undelegations[i], expected_undelegations[i],
            "Delegation is not correct"
        )
    }

    let mut validators = vec![
        default_validator_with_delegations!(10),
        default_validator_with_delegations!(10),
        default_validator_with_delegations!(10),
    ];
    let expected_undelegations: Vec<Uint128> = vec![Uint128(3), Uint128(3), Uint128(4)];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

    let undelegate_amount = Uint128(10);
    let undelegations = calculate_undelegations(undelegate_amount, validators.as_slice()).unwrap();

    assert_eq!(
        validators.len(),
        undelegations.len(),
        "Delegations are not correct"
    );
    for i in 0..expected_undelegations.len() {
        assert_eq!(
            undelegations[i], expected_undelegations[i],
            "Delegation is not correct"
        )
    }

    let mut validators = vec![
        default_validator_with_delegations!(10),
        default_validator_with_delegations!(10),
        default_validator_with_delegations!(10),
    ];
    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

    let undelegate_amount = Uint128(1000);
    if let Some(e) = calculate_undelegations(undelegate_amount, validators.as_slice()).err() {
        assert_eq!(
            e,
            StdError::generic_err("undelegate amount can't be bigger than total delegated amount")
        )
    } else {
        panic!("undelegations invalid")
    }
}

fn set_delegation_query(
    querier: &mut WasmMockQuerier,
    delegate: &[FullDelegation],
    validators: &[CosmosValidator],
) {
    querier.update_staking("uluna", validators, delegate);
}

fn sample_delegation(delegator: HumanAddr, addr: HumanAddr, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = coin(0, &amount.denom);
    FullDelegation {
        validator: addr,
        delegator,
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}
