use cosmwasm_std::{to_binary, Api, Binary, Env, Extern, HandleResponse, InitResponse, Querier, StdError, StdResult, Storage, log, Uint128, Decimal, CosmosMsg, Coin, HumanAddr};

use crate::msg::{HandleMsg, InitMsg, QueryMsg, GetBufferedRewardsResponse};
use crate::state::{read_config, store_config, Config};
use terra_cosmwasm::{SwapResponse, TerraQuerier, TerraMsgWrapper, create_swap_msg};
use std::ops::Mul;

pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    msg: InitMsg,
) -> StdResult<InitResponse> {
    let conf = Config {
        hub_contract: deps.api.canonical_address(&msg.hub_contract)?,
        bluna_reward_denom: msg.bluna_reward_denom,
        stluna_reward_denom: msg.stluna_reward_denom,
    };

    store_config(&mut deps.storage, &conf)?;

    Ok(InitResponse::default())
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    match msg {
        HandleMsg::SwapToRewardDenom {
            bluna_total_bond_amount,
            stluna_total_bond_amount
        } => handle_swap(
            deps,
            env,
            bluna_total_bond_amount,
            stluna_total_bond_amount,
        ),
        HandleMsg::DispatchRewards {} => handle_dispatch_rewards(
            deps,
            env,
        ),
    }
}

pub fn handle_swap<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    bluna_total_bond_amount: Uint128,
    stluna_total_bond_amount: Uint128,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    let config = read_config(&deps.storage)?;
    let owner_addr = deps.api.human_address(&config.hub_contract)?;

    if env.message.sender != owner_addr {
        return Err(StdError::unauthorized());
    }

    let contr_addr = env.contract.address;
    let balance = deps.querier.query_all_balances(contr_addr.clone())?;
    let (
        total_stluna_rewards_available,
        total_bluna_rewards_available,
        mut msgs
    ) = convert_to_target_denoms(
        deps,
        contr_addr.clone(),
        balance,
        config.stluna_reward_denom.clone(),
        config.bluna_reward_denom.clone(),
    )?;

    let (
        stluna_2_bluna_rewards_xchg_rate,
        bluna_2_stluna_rewards_xchg_rate,
    ) = get_exchange_rates(
        deps,
        config.stluna_reward_denom.as_str(),
        config.bluna_reward_denom.as_str())?;

    let (offer_coin, ask_denom) = get_swap_info(
        config,
        stluna_total_bond_amount,
        bluna_total_bond_amount,
        total_stluna_rewards_available,
        total_bluna_rewards_available,
        bluna_2_stluna_rewards_xchg_rate,
        stluna_2_bluna_rewards_xchg_rate,
    ).unwrap();

    msgs.push(create_swap_msg(contr_addr.clone(), offer_coin, ask_denom));
    let res = HandleResponse {
        messages: msgs,
        log: vec![log("action", "swap")],
        data: None,
    };

    Ok(res)
}

pub(crate) fn convert_to_target_denoms<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    contr_addr: HumanAddr,
    balance: Vec<Coin>,
    denom_to_keep: String,
    denom_to_xchg: String,
) -> StdResult<(Uint128, Uint128, Vec<CosmosMsg<TerraMsgWrapper>>)> {
    let terra_querier = TerraQuerier::new(&deps.querier);
    let mut total_luna_available: Uint128 = Uint128::zero();
    let mut total_usd_available: Uint128 = Uint128::zero();

    let mut msgs: Vec<CosmosMsg<TerraMsgWrapper>> = Vec::new();
    for coin in balance {
        if coin.denom == denom_to_keep {
            total_luna_available += coin.amount;
            continue;
        }

        if coin.denom == denom_to_xchg {
            total_usd_available += coin.amount;
            continue;
        }

        let swap_response: SwapResponse = terra_querier.query_swap(
            coin.clone(),
            denom_to_xchg.as_str())?;
        total_usd_available += swap_response.receive.amount;

        msgs.push(create_swap_msg(
            contr_addr.clone(),
            coin,
            denom_to_xchg.to_string(),
        ));
    }

    Ok((total_luna_available, total_usd_available, msgs))
}

pub(crate) fn get_exchange_rates<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    denom_a: &str,
    denom_b: &str,
) -> StdResult<(Decimal, Decimal)> {
    let terra_querier = TerraQuerier::new(&deps.querier);
    let a_2_b_xchg_rates = terra_querier.query_exchange_rates(
        denom_a.to_string(),
        vec![denom_b.to_string()])?.exchange_rates;
    let b_2_a_xchg_rates = terra_querier.query_exchange_rates(
        denom_b.to_string(),
        vec![denom_a.to_string()])?.exchange_rates;

    Ok((a_2_b_xchg_rates[0].exchange_rate, b_2_a_xchg_rates[0].exchange_rate))
}

pub(crate) fn get_swap_info(
    config: Config,
    stluna_total_bond_amount: Uint128,
    bluna_total_bond_amount: Uint128,
    total_stluna_rewards_available: Uint128,
    total_bluna_rewards_available: Uint128,
    bluna_2_stluna_rewards_xchg_rate: Decimal,
    stluna_2_bluna_rewards_xchg_rate: Decimal,
) -> StdResult<(Coin, String)> {
    // Total rewards in stLuna rewards currency.
    let total_rewards_in_stluna_rewards = total_stluna_rewards_available +
        total_bluna_rewards_available.mul(bluna_2_stluna_rewards_xchg_rate);

    let stluna_share_of_total_rewards = total_rewards_in_stluna_rewards.multiply_ratio(
        stluna_total_bond_amount,
        stluna_total_bond_amount + bluna_total_bond_amount,
    );

    if total_stluna_rewards_available.gt(&stluna_share_of_total_rewards) {
        let stluna_rewards_to_sell = (total_stluna_rewards_available -
            stluna_share_of_total_rewards)?;

        Ok((Coin::new(
            stluna_rewards_to_sell.u128(),
            config.stluna_reward_denom.as_str()), config.bluna_reward_denom))
    } else {
        let stluna_rewards_to_buy = (stluna_share_of_total_rewards -
            total_stluna_rewards_available)?;
        let bluna_rewards_to_sell = stluna_rewards_to_buy.mul(
            stluna_2_bluna_rewards_xchg_rate);

        Ok((Coin::new(
            bluna_rewards_to_sell.u128(),
            config.bluna_reward_denom.as_str()), config.stluna_reward_denom))
    }
}

pub fn handle_dispatch_rewards<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
) -> StdResult<HandleResponse<TerraMsgWrapper>> {
    Ok(HandleResponse::default())
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetBufferedRewards {} => to_binary(&query_get_buffered_rewards(deps)?),
    }
}

fn query_get_buffered_rewards<S: Storage, A: Api, Q: Querier>(
    _deps: &Extern<S, A, Q>
) -> StdResult<GetBufferedRewardsResponse> {
    Ok(GetBufferedRewardsResponse {})
}
