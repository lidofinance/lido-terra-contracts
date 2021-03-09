use crate::state::{
    read_config, store_config, store_parameters,
    Parameters,
};
use anchor_basset_reward::msg::HandleMsg::UpdateRewardDenom;
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier,
    StakingMsg, StdError, StdResult, Storage, WasmMsg,
};
use hub_querier::{Registration};

/// Update general parameters
/// Only creator/owner is allowed to execute
#[allow(clippy::too_many_arguments)]
pub fn handle_update_params<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    epoch_period: Option<u64>,
    underlying_coin_denom: Option<String>,
    unbonding_period: Option<u64>,
    peg_recovery_fee: Option<Decimal>,
    er_threshold: Option<Decimal>,
    reward_denom: Option<String>,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.creator {
        return Err(StdError::unauthorized());
    }

    let params: Parameters = store_parameters(&mut deps.storage).load()?;
    let new_params = Parameters {
        epoch_period: epoch_period.unwrap_or(params.epoch_period),
        underlying_coin_denom: underlying_coin_denom.unwrap_or(params.underlying_coin_denom),
        unbonding_period: unbonding_period.unwrap_or(params.unbonding_period),
        peg_recovery_fee: peg_recovery_fee.unwrap_or(params.peg_recovery_fee),
        er_threshold: er_threshold.unwrap_or(params.er_threshold),
        reward_denom: reward_denom.clone().unwrap_or(params.reward_denom),
    };

    let mut msgs: Vec<CosmosMsg> = vec![];
    if let Some(denom) = reward_denom {
        let reward_addr = deps.api.human_address(
            &config
                .reward_contract
                .expect("the reward contract must have been registered"),
        )?;

        // send update denom to the reward contract
        let set_swap = UpdateRewardDenom {
            reward_denom: Some(denom),
        };

        msgs.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: reward_addr,
            msg: to_binary(&set_swap)?,
            send: vec![],
        }));
    }

    store_parameters(&mut deps.storage).save(&new_params)?;
    let res = HandleResponse {
        messages: msgs,
        log: vec![log("action", "update_params")],
        data: None,
    };
    Ok(res)
}

/// Update the config. Update the owner, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    reward_contract: Option<HumanAddr>,
    token_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    // only owner must be able to send this message.
    let conf = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.creator = owner_raw;
            Ok(last_config)
        })?;
    }
    if let Some(reward) = reward_contract {
        let reward_raw = deps.api.canonical_address(&reward)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.reward_contract = Some(reward_raw);
            Ok(last_config)
        })?;

        // register the reward contract for automate reward withdrawal.
        let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
            validator: HumanAddr::default(),
            recipient: Some(reward),
        });
        messages.push(msg);
    }

    if let Some(token) = token_contract {
        let token_raw = deps.api.canonical_address(&token)?;

        store_config(&mut deps.storage).update(|mut last_config| {
            last_config.token_contract = Some(token_raw);
            Ok(last_config)
        })?;
    }

    let res = HandleResponse {
        messages,
        log: vec![log("action", "change_the_owner")],
        data: None,
    };
    Ok(res)
}

/// Register subcontracts, reward and token contracts.
/// Only creator/owner is allowed to execute
pub fn handle_register_contracts<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    contract: Registration,
    contract_address: HumanAddr,
) -> StdResult<HandleResponse> {
    // only owner must be able to send this message.
    let conf = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != conf.creator {
        return Err(StdError::unauthorized());
    }

    let raw_contract_addr = deps.api.canonical_address(&contract_address)?;
    let mut messages: Vec<CosmosMsg> = vec![];

    // if contract is reward, store the contract address for reward in config.
    // if contract is token, store the contract address for token in config.
    match contract {
        Registration::Reward => {
            if conf.reward_contract.is_some() {
                return Err(StdError::generic_err(
                    "The reward contract is already registered",
                ));
            }
            store_config(&mut deps.storage).update(|mut last_config| {
                last_config.reward_contract = Some(raw_contract_addr.clone());
                Ok(last_config)
            })?;

            // register the reward contract for automate reward withdrawal.
            let msg: CosmosMsg = CosmosMsg::Staking(StakingMsg::Withdraw {
                validator: HumanAddr::default(),
                recipient: Some(deps.api.human_address(&raw_contract_addr)?),
            });
            messages.push(msg);
        }
        Registration::Token => {
            store_config(&mut deps.storage).update(|mut last_config| {
                if last_config.token_contract.is_some() {
                    return Err(StdError::generic_err(
                        "The token contract is already registered",
                    ));
                }
                last_config.token_contract = Some(raw_contract_addr.clone());
                Ok(last_config)
            })?;
        },
        Registration::ValidatorsRegistry => {
            store_config(&mut deps.storage).update(|mut last_config| {
                if last_config.validators_registry_contract.is_some() {
                    return Err(StdError::generic_err(
                        "The token contract is already registered",
                    ));
                }
                last_config.validators_registry_contract = Some(raw_contract_addr.clone());
                Ok(last_config)
            })?;
        }
    }
    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "register"),
            log("sub_contract", contract_address),
        ],
        data: None,
    };
    Ok(res)
}
