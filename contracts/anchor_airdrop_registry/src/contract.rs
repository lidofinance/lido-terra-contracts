use crate::msg::{
    AirdropInfoElem, AirdropInfoResponse, ConfigResponse, HandleMsg, InitMsg, MIRAirdropHandleMsg,
    PairHandleMsg, QueryMsg,
};
use crate::state::{
    read_airdrop_info, read_all_airdrop_infos, read_config, remove_airdrop_info,
    store_airdrop_info, store_config, update_airdrop_info, AirdropInfo, Config,
};
use cosmwasm_std::{
    log, to_binary, Api, Binary, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, InitResponse,
    Querier, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use hub_querier::HandleMsg as HubHandleMsg;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let sender = env.message.sender;
    let sndr_raw = deps.api.canonical_address(&sender)?;

    let config = Config {
        owner: sndr_raw,
        hub_contract: msg.hub_contract,
        reward_contract: msg.reward_contract,
        airdrop_tokens: vec![],
    };

    store_config(&mut deps.storage).save(&config)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::FabricateMIRClaim {
            stage,
            amount,
            proof,
        } => handle_fabricate_mir_claim(deps, env, stage, amount, proof),
        HandleMsg::UpdateConfig {
            owner,
            hub_contract,
            reward_contract,
        } => handle_update_config(deps, env, owner, hub_contract, reward_contract),
        HandleMsg::AddAirdropInfo {
            airdrop_token,
            airdrop_info,
        } => handle_add_airdrop(deps, env, airdrop_token, airdrop_info),
        HandleMsg::RemoveAirdropInfo { airdrop_token } => {
            handle_remove_airdrop(deps, env, airdrop_token)
        }
        HandleMsg::UpdateAirdropInfo {
            airdrop_token,
            airdrop_info,
        } => handle_update_airdrop(deps, env, airdrop_token, airdrop_info),
    }
}

fn handle_fabricate_mir_claim<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    stage: u8,
    amount: Uint128,
    proof: Vec<String>,
) -> StdResult<HandleResponse> {
    let config = read_config(&deps.storage).load()?;

    let mut messages: Vec<CosmosMsg> = vec![];

    let airdrop_info = read_airdrop_info(&deps.storage, "MIR".to_string());
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.hub_contract,
        msg: to_binary(&HubHandleMsg::ClaimAirdrop {
            airdrop_token_contract: airdrop_info.airdrop_token_contract,
            airdrop_contract: airdrop_info.airdrop_contract,
            airdrop_swap_contract: airdrop_info.airdrop_swap_contract,
            claim_msg: to_binary(&MIRAirdropHandleMsg::Claim {
                stage,
                amount,
                proof,
            })?,
            swap_msg: to_binary(&PairHandleMsg::Swap {
                belief_price: airdrop_info.swap_belief_price,
                max_spread: airdrop_info.swap_max_spread,
                to: Some(config.reward_contract),
            })?,
        })?,
        send: vec![],
    }));

    Ok(HandleResponse {
        messages,
        log: vec![log("action", "fabricate_mir_claim")],
        data: None,
    })
}

pub fn handle_update_config<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    owner: Option<HumanAddr>,
    hub_contract: Option<HumanAddr>,
    reward_contract: Option<HumanAddr>,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let mut config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.canonical_address(&o)?;
        config.owner = owner_raw
    }
    if let Some(hub) = hub_contract {
        config.hub_contract = hub;
    }
    if let Some(reward_addr) = reward_contract {
        config.reward_contract = reward_addr;
    }

    store_config(&mut deps.storage).save(&config)?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("action", "update_config")],
        data: None,
    })
}

pub fn handle_add_airdrop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.owner {
        return Err(StdError::unauthorized());
    }

    store_config(&mut deps.storage).update(|mut conf| {
        conf.airdrop_tokens.push(airdrop_token.clone());
        Ok(conf)
    })?;

    store_airdrop_info(&mut deps.storage, airdrop_token.clone(), airdrop_info)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "add_airdrop_info"),
            log("airdrop_token", airdrop_token),
        ],
        data: None,
    })
}

pub fn handle_update_airdrop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token: String,
    airdrop_info: AirdropInfo,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.owner {
        return Err(StdError::unauthorized());
    }

    update_airdrop_info(&mut deps.storage, airdrop_token.clone(), airdrop_info)?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "update_airdrop_info"),
            log("airdrop_token", airdrop_token),
        ],
        data: None,
    })
}

pub fn handle_remove_airdrop<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token: String,
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.owner {
        return Err(StdError::unauthorized());
    }

    store_config(&mut deps.storage).update(|mut conf| {
        conf.airdrop_tokens.retain(|item| item != &airdrop_token);
        Ok(conf)
    })?;

    remove_airdrop_info(&mut deps.storage, airdrop_token.clone())?;
    Ok(HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "remove_airdrop_info"),
            log("airdrop_token", airdrop_token),
        ],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(&deps)?),
        QueryMsg::AirdropInfo {
            airdrop_token,
            start_after,
            limit,
        } => to_binary(&query_airdrop_infos(
            &deps,
            airdrop_token,
            start_after,
            limit,
        )?),
    }
}

fn query_config<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<ConfigResponse> {
    let config = read_config(&deps.storage).load()?;
    let owner_addr = deps.api.human_address(&config.owner)?;

    Ok(ConfigResponse {
        owner: owner_addr,
        hub_contract: config.hub_contract,
        reward_contract: config.reward_contract,
        airdrop_tokens: config.airdrop_tokens,
    })
}

fn query_airdrop_infos<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    airdrop_token: Option<String>,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AirdropInfoResponse> {
    if let Some(air_token) = airdrop_token {
        let info = read_airdrop_info(&deps.storage, air_token.clone());

        Ok(AirdropInfoResponse {
            airdrop_info: vec![AirdropInfoElem {
                airdrop_token: air_token,
                info,
            }],
        })
    } else {
        let infos = read_all_airdrop_infos(&deps.storage, start_after, limit)?;
        Ok(AirdropInfoResponse {
            airdrop_info: infos,
        })
    }
}
