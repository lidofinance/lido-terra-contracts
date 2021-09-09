use crate::common::{calculate_delegations, calculate_undelegations};
use crate::contract::{execute, instantiate};
use crate::msg::{ExecuteMsg, InstantiateMsg};
use crate::registry::{Validator, CONFIG, REGISTRY};
use crate::testing::mock_querier::{mock_dependencies, WasmMockQuerier};
use basset::hub::ExecuteMsg::{RedelegateProxy, UpdateGlobalIndex};
use cosmwasm_std::testing::{mock_env, mock_info};
use cosmwasm_std::{
    coin, coins, to_binary, Addr, Api, Coin, CosmosMsg, FullDelegation, StdError, Uint128,
    Validator as CosmosValidator, WasmMsg,
};

#[test]
fn proper_instantiate() {
    let mut deps = mock_dependencies(&[]);

    let hub_address = String::from("hub_contract_address");

    let msg = InstantiateMsg {
        registry: vec![Validator {
            total_delegated: Default::default(),
            address: Default::default(),
        }],
        hub_contract: hub_address.clone(),
    };
    let info = mock_info("creator", &coins(1000, "earth"));

    // we can just call .unwrap() to assert this was a success
    let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    assert_eq!(
        CONFIG.load(&deps.storage).unwrap().hub_contract,
        deps.api.addr_canonicalize(&hub_address).unwrap()
    )
}

#[test]
fn add_validator() {
    let mut deps = mock_dependencies(&coins(2, "token"));

    let msg = InstantiateMsg {
        registry: vec![],
        hub_contract: String::from("hub_contract_address"),
    };
    let info = mock_info("creator", &coins(2, "token"));
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    let validator = Validator {
        total_delegated: Default::default(),
        address: Default::default(),
    };

    let msg = ExecuteMsg::AddValidator {
        validator: validator.clone(),
    };
    let _res = execute(deps.as_mut(), mock_env(), info, msg);

    match _res {
        Ok(_) => {
            let v = REGISTRY
                .load(&deps.storage, validator.address.as_str().as_bytes())
                .unwrap();
            assert_eq!(validator, v);
        }
        Err(e) => panic!("Failed to handle AddValidator message: {}", e),
    }
}

#[test]
fn ownership_tests() {
    let mut deps = mock_dependencies(&coins(2, "token"));

    let msg = InstantiateMsg {
        registry: vec![],
        hub_contract: String::from("hub_contract_address"),
    };
    let info = mock_info("creator", &coins(2, "token"));
    let _res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

    let info = mock_info("villain", &coins(2, "token"));

    let validator = Validator {
        total_delegated: Default::default(),
        address: Default::default(),
    };

    let msg = ExecuteMsg::AddValidator {
        validator: validator.clone(),
    };
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    assert_eq!(res.err().unwrap(), StdError::generic_err("unauthorized"));

    let msg = ExecuteMsg::RemoveValidator {
        address: validator.address,
    };
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    assert_eq!(res.err().unwrap(), StdError::generic_err("unauthorized"));

    let msg = ExecuteMsg::UpdateConfig {
        hub_contract: None,
        owner: None,
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    assert_eq!(res.err().unwrap(), StdError::generic_err("unauthorized"));
}

#[test]
fn update_config() {
    let mut deps = mock_dependencies(&coins(2, "token"));

    let msg = InstantiateMsg {
        registry: vec![],
        hub_contract: String::from("hub_contract_address"),
    };
    let info = mock_info("creator", &coins(2, "token"));
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    let new_hub_address = String::from("new_hub_contract");
    let msg = ExecuteMsg::UpdateConfig {
        hub_contract: Some(new_hub_address.clone()),
        owner: None,
    };
    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    assert!(res.is_ok());
    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(
        deps.api.addr_canonicalize(&new_hub_address).unwrap(),
        config.hub_contract
    );

    let new_owner = String::from("new_owner");
    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(new_owner.clone()),
        hub_contract: None,
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    assert!(res.is_ok());
    let config = CONFIG.load(&deps.storage).unwrap();
    assert_eq!(
        deps.api.addr_canonicalize(&new_owner).unwrap(),
        config.owner
    );
}

#[test]
fn remove_validator() {
    let mut deps = mock_dependencies(&coins(2, "token"));
    let hub_contract_address = deps
        .api
        .addr_validate(&String::from("hub_contract_address"))
        .unwrap();
    let validator1 = Validator {
        total_delegated: Uint128::zero(),
        address: String::from("validator"),
    };

    let validator2 = Validator {
        total_delegated: Uint128::zero(),
        address: String::from("validator2"),
    };

    let validator3 = Validator {
        total_delegated: Uint128::zero(),
        address: String::from("validator3"),
    };

    let validator4 = Validator {
        total_delegated: Uint128::zero(),
        address: String::from("validator4"),
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
    let msg = InstantiateMsg {
        registry: vec![
            validator1.clone(),
            validator2.clone(),
            validator3.clone(),
            validator4.clone(),
        ],
        hub_contract: hub_contract_address.to_string(),
    };

    let info = mock_info("creator", &coins(2, "token"));
    let _res = instantiate(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();

    // try to remove validator4
    let msg = ExecuteMsg::RemoveValidator {
        address: validator4.address.clone(),
    };
    let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = REGISTRY.load(&deps.storage, validator4.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&RedelegateProxy {
                            src_validator: validator4.address.clone(),
                            redelegations: vec![
                                (validator1.clone().address, coin(27, "uluna")),
                                (validator2.clone().address, coin(17, "uluna")),
                                (validator3.clone().address, coin(6, "uluna"))
                            ]
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(contract_addr, hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[1];
            match update_global_index.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(*contract_addr, String::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!("Failed to handle RemoveValidator message: {}", e),
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
    let msg = ExecuteMsg::RemoveValidator {
        address: validator3.address.clone(),
    };
    let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = REGISTRY.load(&deps.storage, validator3.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&RedelegateProxy {
                            src_validator: validator3.address.clone(),
                            redelegations: vec![
                                (validator1.clone().address, coin(18, "uluna")),
                                (validator2.clone().address, coin(18, "uluna"))
                            ]
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(contract_addr, hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[1];
            match update_global_index.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(*contract_addr, String::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!("Failed to handle RemoveValidator message: {}", e),
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
    let msg = ExecuteMsg::RemoveValidator {
        address: validator2.address.clone(),
    };
    let _res = execute(deps.as_mut(), mock_env(), info.clone(), msg);
    match _res {
        Ok(res) => {
            let reg = REGISTRY.load(&deps.storage, validator2.address.as_str().as_bytes());
            assert!(reg.is_err(), "Validator was not removed");

            let redelegate = &res.messages[0];
            match redelegate.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&RedelegateProxy {
                            src_validator: validator2.address,
                            redelegations: vec![(validator1.clone().address, coin(55, "uluna"))],
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(contract_addr, hub_contract_address.to_string());
                }
                _ => panic!("Unexpected message: {:?}", redelegate),
            }

            let update_global_index = &res.messages[1];
            match update_global_index.msg.clone() {
                CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr,
                    msg,
                    funds: _,
                }) => {
                    assert_eq!(
                        *msg.0,
                        to_binary(&UpdateGlobalIndex {
                            airdrop_hooks: None
                        })
                        .unwrap()
                        .0
                    );
                    assert_eq!(*contract_addr, String::from("hub_contract_address"));
                }
                _ => panic!("Unexpected message: {:?}", update_global_index),
            }
        }
        Err(e) => panic!("Failed to handle RemoveValidator message: {}", e),
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
    let msg = ExecuteMsg::RemoveValidator {
        address: validator1.address,
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg);
    assert_eq!(
        res.expect_err("The last validator was removed from registry"),
        StdError::generic_err("Cannot remove the last validator in the registry",)
    );
}

#[macro_export]
macro_rules! default_validator_with_delegations {
    ($total:expr) => {
        Validator {
            total_delegated: Uint128::from($total as u128),
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
    let expected_delegations: Vec<Uint128> = vec![
        Uint128::from(4u128),
        Uint128::from(3u128),
        Uint128::from(3u128),
    ];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

    let buffered_balance = Uint128::from(10u128);
    let (remained_balance, delegations) =
        calculate_delegations(buffered_balance, validators.as_slice()).unwrap();

    assert_eq!(
        validators.len(),
        delegations.len(),
        "Delegations are not correct"
    );
    assert_eq!(
        remained_balance,
        Uint128::zero(),
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
    let expected_undelegations: Vec<Uint128> = vec![
        Uint128::from(93u128),
        Uint128::from(3u128),
        Uint128::from(4u128),
    ];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

    let undelegate_amount = Uint128::from(100u128);
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
    let expected_undelegations: Vec<Uint128> = vec![
        Uint128::from(3u128),
        Uint128::from(3u128),
        Uint128::from(4u128),
    ];

    // sort validators for the right delegations
    validators.sort_by(|v1, v2| v2.total_delegated.cmp(&v1.total_delegated));

    let undelegate_amount = Uint128::from(10u128);
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

    let undelegate_amount = Uint128::from(1000u128);
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

fn sample_delegation(delegator: Addr, addr: String, amount: Coin) -> FullDelegation {
    let can_redelegate = amount.clone();
    let accumulated_rewards = vec![coin(0, &amount.denom)];
    FullDelegation {
        validator: addr,
        delegator,
        amount,
        can_redelegate,
        accumulated_rewards,
    }
}
