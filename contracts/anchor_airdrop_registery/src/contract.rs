use crate::msg::{
    AirdropInfoResponse, AirdropInfosResponse, ConfigResponse, HandleMsg, InitMsg,
    MIRAirdropHandleMsg, QueryMsg,
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
        airdrop_tokens: msg.airdrop_tokens,
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
            hub_contract,
            owner,
        } => handle_update_config(deps, env, owner, hub_contract),
        HandleMsg::AddAirdropToken {
            airdrop_token,
            airdrop_info,
        } => handle_add_airdrop(deps, env, airdrop_token, airdrop_info),
        HandleMsg::RemoveAirdropToken { airdrop_token } => {
            handle_remove_airdrop(deps, env, airdrop_token)
        }
        HandleMsg::UpdateAirdropToken {
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
            claim_msg: to_binary(&MIRAirdropHandleMsg::Claim {
                stage,
                amount,
                proof,
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
) -> StdResult<HandleResponse> {
    // only owner can send this message.
    let mut config = read_config(&deps.storage).load()?;
    let sender_raw = deps.api.canonical_address(&env.message.sender)?;
    if sender_raw != config.owner {
        return Err(StdError::unauthorized());
    }

    if let Some (o) = owner{
        let owner_raw = deps.api.canonical_address(&o)?;
        config.owner = owner_raw
    }
    if let Some (hub) = hub_contract{
        config.hub_contract = hub;
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
        QueryMsg::AirdropInfo { airdrop_token } => {
            to_binary(&query_airdrop_info(&deps, airdrop_token)?)
        }
        QueryMsg::AirdropInfos {} => to_binary(&query_airdrop_infos(&deps)?),
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
        airdrop_tokens: config.airdrop_tokens,
    })
}

fn query_airdrop_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    airdrop_token: String,
) -> StdResult<AirdropInfoResponse> {
    let info = read_airdrop_info(&deps.storage, airdrop_token);

    Ok(AirdropInfoResponse {
        airdrop_token_contract: info.airdrop_token_contract,
        airdrop_contract: info.airdrop_contract,
    })
}

fn query_airdrop_infos<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<AirdropInfosResponse> {
    let infos = read_all_airdrop_infos(&deps.storage)?;
    Ok(AirdropInfosResponse { infos })
}
