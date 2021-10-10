#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;

use cosmwasm_std::{
    attr, to_binary, Attribute, BankMsg, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Response, StdError, StdResult, Uint128, WasmMsg,
};

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG};
use basset::hub::ExecuteMsg::{BondRewards, UpdateGlobalIndex};
use basset::{compute_lido_fee, deduct_tax};
use std::ops::Mul;
use terra_cosmwasm::{create_swap_msg, SwapResponse, TerraMsgWrapper, TerraQuerier};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let conf = Config {
        owner: deps.api.addr_canonicalize(info.sender.as_str())?,
        hub_contract: deps.api.addr_canonicalize(&msg.hub_contract)?,
        bluna_reward_contract: deps.api.addr_canonicalize(&msg.bluna_reward_contract)?,
        bluna_reward_denom: msg.bluna_reward_denom,
        stluna_reward_denom: msg.stluna_reward_denom,
        lido_fee_address: deps.api.addr_canonicalize(&msg.lido_fee_address)?,
        lido_fee_rate: msg.lido_fee_rate,
    };

    CONFIG.save(deps.storage, &conf)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response<TerraMsgWrapper>> {
    match msg {
        ExecuteMsg::SwapToRewardDenom {
            bluna_total_bonded: bluna_total_mint_amount,
            stluna_total_bonded: stluna_total_mint_amount,
        } => execute_swap(
            deps,
            env,
            info,
            bluna_total_mint_amount,
            stluna_total_mint_amount,
        ),
        ExecuteMsg::DispatchRewards {} => execute_dispatch_rewards(deps, env, info),
        ExecuteMsg::UpdateConfig {
            owner,
            hub_contract,
            bluna_reward_contract,
            stluna_reward_denom,
            bluna_reward_denom,
            lido_fee_address,
            lido_fee_rate,
        } => execute_update_config(
            deps,
            env,
            info,
            owner,
            hub_contract,
            bluna_reward_contract,
            stluna_reward_denom,
            bluna_reward_denom,
            lido_fee_address,
            lido_fee_rate,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute_update_config(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    owner: Option<String>,
    hub_contract: Option<String>,
    bluna_reward_contract: Option<String>,
    stluna_reward_denom: Option<String>,
    bluna_reward_denom: Option<String>,
    lido_fee_address: Option<String>,
    lido_fee_rate: Option<Decimal>,
) -> StdResult<Response<TerraMsgWrapper>> {
    let conf = CONFIG.load(deps.storage)?;
    let sender_raw = deps.api.addr_canonicalize(info.sender.as_str())?;
    if sender_raw != conf.owner {
        return Err(StdError::generic_err("unauthorized"));
    }

    if let Some(o) = owner {
        let owner_raw = deps.api.addr_canonicalize(&o)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.owner = owner_raw;
            Ok(last_config)
        })?;
    }

    if let Some(h) = hub_contract {
        let hub_raw = deps.api.addr_canonicalize(&h)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.hub_contract = hub_raw;
            Ok(last_config)
        })?;
    }

    if let Some(b) = bluna_reward_contract {
        let bluna_raw = deps.api.addr_canonicalize(&b)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.bluna_reward_contract = bluna_raw;
            Ok(last_config)
        })?;
    }

    if let Some(s) = stluna_reward_denom {
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.stluna_reward_denom = s;
            Ok(last_config)
        })?;
    }

    if let Some(b) = bluna_reward_denom {
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.bluna_reward_denom = b;
            Ok(last_config)
        })?;
    }

    if let Some(r) = lido_fee_rate {
        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.lido_fee_rate = r;
            Ok(last_config)
        })?;
    }

    if let Some(a) = lido_fee_address {
        let address_raw = deps.api.addr_canonicalize(&a)?;

        CONFIG.update(deps.storage, |mut last_config| -> StdResult<_> {
            last_config.lido_fee_address = address_raw;
            Ok(last_config)
        })?;
    }

    Ok(Response::default())
}

pub fn execute_swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    bluna_total_bonded_amount: Uint128,
    stluna_total_bonded_amount: Uint128,
) -> StdResult<Response<TerraMsgWrapper>> {
    let config = CONFIG.load(deps.storage)?;
    let hub_addr = deps.api.addr_humanize(&config.hub_contract)?;

    if info.sender != hub_addr {
        return Err(StdError::generic_err("unauthorized"));
    }

    let contr_addr = env.contract.address;
    let balance = deps.querier.query_all_balances(contr_addr.clone())?;
    let (total_luna_rewards_available, total_ust_rewards_available, mut msgs) =
        convert_to_target_denoms(
            &deps,
            contr_addr.to_string(),
            balance.clone(),
            config.stluna_reward_denom.clone(),
            config.bluna_reward_denom.clone(),
        )?;

    let (luna_2_ust_rewards_xchg_rate, ust_2_luna_rewards_xchg_rate) = get_exchange_rates(
        &deps,
        config.stluna_reward_denom.as_str(),
        config.bluna_reward_denom.as_str(),
    )?;

    let (offer_coin, ask_denom) = get_swap_info(
        config,
        stluna_total_bonded_amount,
        bluna_total_bonded_amount,
        total_luna_rewards_available,
        total_ust_rewards_available,
        ust_2_luna_rewards_xchg_rate,
        luna_2_ust_rewards_xchg_rate,
    )?;

    if !offer_coin.amount.is_zero() {
        msgs.push(create_swap_msg(offer_coin.clone(), ask_denom.clone()));
    }

    let res = Response::new().add_messages(msgs).add_attributes(vec![
        attr("action", "swap"),
        attr("initial_balance", format!("{:?}", balance)),
        attr(
            "luna_2_ust_rewards_xchg_rate",
            luna_2_ust_rewards_xchg_rate.to_string(),
        ),
        attr(
            "ust_2_luna_rewards_xchg_rate",
            ust_2_luna_rewards_xchg_rate.to_string(),
        ),
        attr("total_luna_rewards_available", total_luna_rewards_available),
        attr("total_ust_rewards_available", total_ust_rewards_available),
        attr("offer_coin_denom", offer_coin.denom),
        attr("offer_coin_amount", offer_coin.amount),
        attr("ask_denom", ask_denom),
    ]);

    Ok(res)
}

pub(crate) fn convert_to_target_denoms(
    deps: &DepsMut,
    _contr_addr: String,
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

        let swap_response: SwapResponse =
            terra_querier.query_swap(coin.clone(), denom_to_xchg.as_str())?;
        total_usd_available += swap_response.receive.amount;

        msgs.push(create_swap_msg(coin, denom_to_xchg.to_string()));
    }

    Ok((total_luna_available, total_usd_available, msgs))
}

pub(crate) fn get_exchange_rates(
    deps: &DepsMut,
    denom_a: &str,
    denom_b: &str,
) -> StdResult<(Decimal, Decimal)> {
    let terra_querier = TerraQuerier::new(&deps.querier);
    let a_2_b_xchg_rates = terra_querier
        .query_exchange_rates(denom_a.to_string(), vec![denom_b.to_string()])?
        .exchange_rates;

    let b_2_a_xchg_rates = terra_querier
        .query_exchange_rates(denom_b.to_string(), vec![denom_a.to_string()])?
        .exchange_rates;

    Ok((
        a_2_b_xchg_rates[0].exchange_rate,
        b_2_a_xchg_rates[0].exchange_rate,
    ))
}

pub(crate) fn get_swap_info(
    config: Config,
    stluna_total_bonded_amount: Uint128,
    bluna_total_bonded_amount: Uint128,
    total_stluna_rewards_available: Uint128,
    total_bluna_rewards_available: Uint128,
    bluna_2_stluna_rewards_xchg_rate: Decimal,
    stluna_2_bluna_rewards_xchg_rate: Decimal,
) -> StdResult<(Coin, String)> {
    // Total rewards in stLuna rewards currency.
    let total_rewards_in_stluna_rewards = total_stluna_rewards_available
        + total_bluna_rewards_available.mul(bluna_2_stluna_rewards_xchg_rate);

    let stluna_share_of_total_rewards = total_rewards_in_stluna_rewards.multiply_ratio(
        stluna_total_bonded_amount,
        stluna_total_bonded_amount + bluna_total_bonded_amount,
    );

    if total_stluna_rewards_available.gt(&stluna_share_of_total_rewards) {
        let stluna_rewards_to_sell =
            total_stluna_rewards_available.checked_sub(stluna_share_of_total_rewards)?;

        Ok((
            Coin::new(
                stluna_rewards_to_sell.u128(),
                config.stluna_reward_denom.as_str(),
            ),
            config.bluna_reward_denom,
        ))
    } else {
        let stluna_rewards_to_buy =
            stluna_share_of_total_rewards.checked_sub(total_stluna_rewards_available)?;
        let bluna_rewards_to_sell = stluna_rewards_to_buy.mul(stluna_2_bluna_rewards_xchg_rate);

        Ok((
            Coin::new(
                bluna_rewards_to_sell.u128(),
                config.bluna_reward_denom.as_str(),
            ),
            config.stluna_reward_denom,
        ))
    }
}

pub fn execute_dispatch_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> StdResult<Response<TerraMsgWrapper>> {
    let config = CONFIG.load(deps.storage)?;

    let hub_addr = deps.api.addr_humanize(&config.hub_contract)?;
    if info.sender != hub_addr {
        return Err(StdError::generic_err("unauthorized"));
    }

    let bluna_reward_addr = deps.api.addr_humanize(&config.bluna_reward_contract)?;

    let contr_addr = env.contract.address;
    let mut stluna_rewards = deps
        .querier
        .query_balance(contr_addr.clone(), config.stluna_reward_denom.as_str())?;
    let lido_stluna_fee_amount = compute_lido_fee(stluna_rewards.amount, config.lido_fee_rate)?;
    stluna_rewards.amount = stluna_rewards.amount.checked_sub(lido_stluna_fee_amount)?;

    let mut bluna_rewards = deps
        .querier
        .query_balance(contr_addr, config.bluna_reward_denom.as_str())?;
    let lido_bluna_fee_amount = compute_lido_fee(bluna_rewards.amount, config.lido_fee_rate)?;
    bluna_rewards.amount = bluna_rewards.amount.checked_sub(lido_bluna_fee_amount)?;

    let mut fees_attrs: Vec<Attribute> = vec![];

    let mut lido_fees: Vec<Coin> = vec![];
    if !lido_stluna_fee_amount.is_zero() {
        let stluna_fee = deduct_tax(
            &deps.querier,
            Coin {
                amount: lido_stluna_fee_amount,
                denom: stluna_rewards.denom.clone(),
            },
        )?;
        if !stluna_fee.amount.is_zero() {
            lido_fees.push(stluna_fee.clone());
            fees_attrs.push(attr("lido_stluna_fee", stluna_fee.to_string()));
        }
    }
    if !lido_bluna_fee_amount.is_zero() {
        let bluna_fee = deduct_tax(
            &deps.querier,
            Coin {
                amount: lido_bluna_fee_amount,
                denom: bluna_rewards.denom.clone(),
            },
        )?;
        if !bluna_fee.amount.is_zero() {
            lido_fees.push(bluna_fee.clone());
            fees_attrs.push(attr("lido_bluna_fee", bluna_fee.to_string()));
        }
    }

    let mut messages: Vec<CosmosMsg<TerraMsgWrapper>> = vec![];
    if !stluna_rewards.amount.is_zero() {
        stluna_rewards = deduct_tax(&deps.querier, stluna_rewards)?;
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hub_addr.to_string(),
            msg: to_binary(&BondRewards {}).unwrap(),
            funds: vec![stluna_rewards.clone()],
        }));
    }
    if !lido_fees.is_empty() {
        messages.push(
            BankMsg::Send {
                to_address: deps
                    .api
                    .addr_humanize(&config.lido_fee_address)?
                    .to_string(),
                amount: lido_fees,
            }
            .into(),
        )
    }
    if !bluna_rewards.amount.is_zero() {
        bluna_rewards = deduct_tax(&deps.querier, bluna_rewards)?;
        messages.push(
            BankMsg::Send {
                to_address: bluna_reward_addr.to_string(),
                amount: vec![bluna_rewards.clone()],
            }
            .into(),
        )
    }
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: bluna_reward_addr.to_string(),
        msg: to_binary(&UpdateGlobalIndex {
            airdrop_hooks: None,
        })
        .unwrap(),
        funds: vec![],
    }));

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![
            attr("action", "claim_reward"),
            attr("bluna_reward_addr", bluna_reward_addr),
            attr("stluna_rewards", stluna_rewards.to_string()),
            attr("bluna_rewards", bluna_rewards.to_string()),
        ])
        .add_attributes(fees_attrs))
}

fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::GetBufferedRewards {} => unimplemented!(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
