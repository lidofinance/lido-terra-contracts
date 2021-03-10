use cosmwasm_std::{
    to_binary, Api, Binary, Env, Extern, HandleResponse, HumanAddr, InitResponse, Querier,
    StdError, StdResult, Storage,
};

use crate::msg::{HandleMsg, InitMsg, QueryMsg};
use crate::registry::{registry, registry_read, Validator};

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
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
    }
}

pub fn update_total_delegated<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    updated_validators: Vec<Validator>,
) -> StdResult<HandleResponse> {
    for validator in updated_validators.iter() {
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
    Ok(HandleResponse::default())
}

pub fn add_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    validator: Validator,
) -> StdResult<HandleResponse> {
    registry(&mut deps.storage).save(validator.address.as_str().as_bytes(), &validator)?;
    Ok(HandleResponse::default())
}

pub fn remove_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    validator_address: HumanAddr,
) -> StdResult<HandleResponse> {
    registry(&mut deps.storage).remove(validator_address.as_str().as_bytes());
    Ok(HandleResponse::default())
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
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, Uint128};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(20, &[]);

        let msg = InitMsg {
            registry: vec![Validator {
                active: true,
                total_delegated: Default::default(),
                address: Default::default(),
            }],
        };
        let env = mock_env("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = init(&mut deps, env, msg).unwrap();
        assert_eq!(0, res.messages.len());
    }

    #[test]
    fn add_validator() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let msg = InitMsg { registry: vec![] };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        // beneficiary can release it
        let env = mock_env("anyone", &coins(2, "token"));

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
    fn remove_validator() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let validator = Validator {
            active: true,
            total_delegated: Default::default(),
            address: Default::default(),
        };

        let msg = InitMsg {
            registry: vec![validator.clone()],
        };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        // beneficiary can release it
        let env = mock_env("anyone", &coins(2, "token"));

        let msg = HandleMsg::RemoveValidator {
            address: validator.address.clone(),
        };
        let _res = handle(&mut deps, env, msg);

        match _res {
            Ok(_) => {
                let reg = registry_read(&deps.storage).load(validator.address.as_str().as_bytes());
                assert!(reg.is_err(), "Validator was not removed");
            }
            Err(e) => panic!(format!("Failed to handle RemoveValidator message: {}", e)),
        }
    }

    #[test]
    fn update_total_delegated() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

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
        };
        let env = mock_env("creator", &coins(2, "token"));
        let _res = init(&mut deps, env, msg).unwrap();

        // beneficiary can release it
        let env = mock_env("anyone", &coins(2, "token"));

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
    }
}
