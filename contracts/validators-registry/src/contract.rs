use cosmwasm_std::{
    to_binary, Api, Binary, Coin, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, WasmMsg,
};

use crate::common::calculate_delegations;
use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::registry::{
    config, config_read, registry, registry_read, store_config, Config, Validator,
};
use hub_querier::HandleMsg::{RedelegateProxy, UpdateGlobalIndex};
use std::ops::AddAssign;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    config(&mut deps.storage).save(&Config {
        owner: deps.api.canonical_address(&env.message.sender)?,
        hub_contract: deps.api.canonical_address(&msg.hub_contract)?,
    })?;

    for v in msg.registry {
        registry(&mut deps.storage).save(v.address.as_str().as_bytes(), &v)?;
    }

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::AddValidator { validator } => add_validator(deps, env, validator),
        HandleMsg::RemoveValidator { address } => remove_validator(deps, env, address),
        HandleMsg::UpdateTotalDelegated { updated_validators } => {
            update_total_delegated(deps, env, updated_validators)
        }
        HandleMsg::UpdateConfig {
            owner,
            hub_contract,
        } => handle_update_config(deps, env, owner, hub_contract),
    }
}

/// Update the config. Update the owner and hub contract address.
/// Only creator/owner is allowed to execute
pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    hub_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    // only owner must be able to send this message.
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.owner = owner_raw;
            Ok(last_config)
        })?;
    }

    if let Some(hub) = hub_contract {
        let hub_raw = deps.api.canonical_address(&hub)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.hub_contract = hub_raw;
            Ok(last_config)
        })?;
    }

    Ok(HandleResponse::default())
}

fn _update_total_delegated<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    validators: &[Validator],
) -> StdResult<()> {
    for validator in validators.iter() {
        registry(&mut deps.storage).update(validator.address.as_str().as_bytes(), |v| match v {
            None => Err(StdError::NotFound {
                kind: validator.address.to_string(),
                backtrace: None,
            }),
            Some(v) => Ok(Validator {
                total_delegated: validator.total_delegated,
                ..v
            }),
        })?;
    }
    Ok(())
}

pub fn update_total_delegated<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    updated_validators: Vec<Validator>,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let hub_address = deps.api.human_address(&config.hub_contract)?;
    if env.message.sender != hub_address {
        return Err(StdError::unauthorized());
    }

    _update_total_delegated(deps, updated_validators.as_slice())?;

    Ok(HandleResponse::default())
}

pub fn add_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: Validator,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    registry(&mut deps.storage).save(validator.address.as_str().as_bytes(), &validator)?;
    Ok(HandleResponse::default())
}

pub fn remove_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator_address: HumanAddr,
) -> StdResult<HandleResponse> {
    let config = config_read(&deps.storage).load()?;
    let owner_address = deps.api.human_address(&config.owner)?;
    if env.message.sender != owner_address {
        return Err(StdError::unauthorized());
    }

    registry(&mut deps.storage).remove(validator_address.as_str().as_bytes());

    let config = config_read(&deps.storage).load()?;
    let hub_address = deps.api.human_address(&config.hub_contract)?;

    let query = deps
        .querier
        .query_delegation(hub_address.clone(), validator_address.clone());

    let mut messages: Vec<CosmosMsg> = vec![];
    if let Ok(q) = query {
        let delegated_amount = q;
        let mut validators = query_validators(deps)?;
        validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));

        if let Some(delegation) = delegated_amount {
            let (_, delegations) =
                calculate_delegations(delegation.amount.amount, validators.as_slice())?;

            for i in 0..delegations.len() {
                if delegations[i].is_zero() {
                    continue;
                }
                let regelegate_msg = RedelegateProxy {
                    src_validator: validator_address.clone(),
                    dst_validator: validators[i].address.clone(),
                    amount: Coin::new(delegations[i].u128(), delegation.amount.denom.as_str()),
                };
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: hub_address.clone(),
                    msg: to_binary(&regelegate_msg)?,
                    send: vec![],
                }));
                validators[i].total_delegated.add_assign(delegations[i]);
            }
            _update_total_delegated(deps, validators.as_slice())?;

            let msg = UpdateGlobalIndex {
                airdrop_hooks: None,
            };
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: hub_address,
                msg: to_binary(&msg)?,
                send: vec![],
            }));
        }
    }

    let res = HandleResponse {
        messages,
        data: None,
        log: vec![],
    };

    Ok(res)
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetValidatorsForDelegation {} => {
            let mut validators = query_validators(deps)?;
            validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));
            to_binary(&validators)
        }
    }
}

fn query_validators<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Vec<Validator>> {
    let mut validators: Vec<Validator> = vec![];
    let registry = registry_read(&deps.storage);
    for key in registry.range(None, None, cosmwasm_std::Order::Ascending) {
        validators.push(key?.1);
    }
    Ok(validators)
}

//TODO: implement
#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::calculate_undelegations;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{
        coin, coins, to_binary, FullDelegation, Uint128, Validator as CosmosValidator,
    };

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(20, &[]);

        let hub_address = HumanAddr::from("hub_contract_address");

        let msg = InitMsg {
            registry: vec![Validator {
                active: true,
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
            active: true,
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
            active: true,
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
            active: true,
            total_delegated: Uint128(10),
            address: HumanAddr::from("validator"),
        };

        let validator2 = Validator {
            active: true,
            total_delegated: Uint128(20),
            address: HumanAddr::from("validator2"),
        };

        let validator3 = Validator {
            active: true,
            total_delegated: Uint128(30),
            address: HumanAddr::from("validator3"),
        };

        let validator4 = Validator {
            active: true,
            total_delegated: Uint128(50),
            address: HumanAddr::from("validator4"),
        };

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

        deps.querier.update_staking(
            "uluna",
            &[CosmosValidator {
                address: validator4.address.clone(),
                commission: Default::default(),
                max_commission: Default::default(),
                max_change_rate: Default::default(),
            }],
            &[FullDelegation {
                delegator: hub_contract_address.clone(),
                validator: validator4.address.clone(),
                amount: Coin {
                    denom: "uluna".to_string(),
                    amount: validator4.total_delegated,
                },
                can_redelegate: Default::default(),
                accumulated_rewards: Default::default(),
            }],
        );

        let msg = HandleMsg::RemoveValidator {
            address: validator4.address.clone(),
        };
        let _res = handle(&mut deps, env, msg);
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
                                dst_validator: validator1.address,
                                amount: coin(27, "uluna"),
                            })
                            .unwrap()
                        );
                        assert_eq!(
                            contract_addr.to_string(),
                            hub_contract_address.clone().to_string()
                        );
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
                                dst_validator: validator2.address,
                                amount: coin(17, "uluna"),
                            })
                            .unwrap()
                        );
                        assert_eq!(
                            contract_addr.to_string(),
                            hub_contract_address.clone().to_string()
                        );
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
                                src_validator: validator4.address.clone(),
                                dst_validator: validator3.address,
                                amount: coin(6, "uluna"),
                            })
                            .unwrap()
                        );
                        assert_eq!(
                            contract_addr.to_string(),
                            hub_contract_address.clone().to_string()
                        );
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
    }

    #[test]
    fn update_total_delegated() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let hub_address = HumanAddr::from("hub_contract_address");

        let validator = Validator {
            active: true,
            total_delegated: Default::default(),
            address: cosmwasm_std::HumanAddr::from("test_validator1"),
        };
        let validator1 = Validator {
            active: true,
            total_delegated: Default::default(),
            address: cosmwasm_std::HumanAddr::from("test_validator2"),
        };

        let msg = InitMsg {
            registry: vec![validator.clone(), validator1.clone()],
            hub_contract: hub_address.clone(),
        };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        let env = mock_env(hub_address, &[]);

        let updated_validator = Validator {
            total_delegated: Uint128(1483),
            ..validator
        };
        let updated_validator1 = Validator {
            total_delegated: Uint128(2244),
            ..validator1
        };

        let updated_validators = vec![updated_validator, updated_validator1];
        let msg = HandleMsg::UpdateTotalDelegated {
            updated_validators: updated_validators.clone(),
        };

        let _res = handle(&mut deps, env, msg);

        match _res {
            Ok(_) => {
                let reg = registry_read(&deps.storage);
                for v in updated_validators {
                    assert_eq!(
                        reg.load(v.address.as_str().as_bytes()).unwrap(),
                        v,
                        "Validators were not updated"
                    );
                }
            }
            Err(e) => panic!(format!("Failed to handle RemoveValidator message: {}", e)),
        }

        // send update from non-hub address
        let env = mock_env("not_hub_address", &[]);

        let msg = HandleMsg::UpdateTotalDelegated {
            updated_validators: vec![],
        };
        assert_eq!(
            handle(&mut deps, env, msg).unwrap_err(),
            StdError::unauthorized()
        );
    }

    #[macro_export]
    macro_rules! default_validator_with_delegations {
        ($total:expr) => {
            Validator {
                active: false,
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
        let undelegations =
            calculate_undelegations(undelegate_amount, validators.as_slice()).unwrap();

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
        let undelegations =
            calculate_undelegations(undelegate_amount, validators.as_slice()).unwrap();

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
                StdError::generic_err(
                    "undelegate amount can't be bigger than total delegated amount"
                )
            )
        } else {
            panic!("undelegations invalid")
        }
    }
}
