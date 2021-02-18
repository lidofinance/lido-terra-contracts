use crate::contract::{query_total_issued, slashing};
use crate::math::decimal_division;
use crate::state::{
    is_valid_validator, read_config, read_current_batch, read_parameters, read_state, store_state,
};
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Env, Extern, HandleResponse, HumanAddr, Querier, StakingMsg,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;

pub fn handle_bond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    // validator must be whitelisted
    let is_valid = is_valid_validator(&deps.storage, validator.clone())?;
    if !is_valid {
        return Err(StdError::generic_err(
            "The chosen validator is currently not supported",
        ));
    }

    let params = read_parameters(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    // current batch requested fee is need for accurate exchange rate computation.
    let current_batch = read_current_batch(&deps.storage).load()?;
    let requested_with_fee = current_batch.requested_with_fee;

    // coin must have be sent along with transaction and it should be in underlying coin denom
    if env.message.sent_funds.len() > 1usize {
        return Err(StdError::generic_err(
            "More than one coin is sent; only one asset is supported",
        ));
    }

    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| {
            StdError::generic_err(format!("No {} assets are provided to bond", coin_denom))
        })?;

    // check slashing
    slashing(deps, env.clone())?;

    let state = read_state(&deps.storage).load()?;
    let sender = env.message.sender.clone();

    // get the total supply
    let mut total_supply = query_total_issued(&deps).unwrap_or_default();

    // peg recovery fee should be considered
    let mint_amount = decimal_division(payment.amount, state.exchange_rate);
    let mut mint_amount_with_fee = mint_amount;
    if state.exchange_rate < threshold {
        let max_peg_fee = mint_amount * recovery_fee;
        let required_peg_fee = ((total_supply + mint_amount + current_batch.requested_with_fee)
            - (state.total_bond_amount + payment.amount))?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        mint_amount_with_fee = (mint_amount - peg_fee)?;
    }

    // total supply should be updated for exchange rate calculation.
    total_supply += mint_amount_with_fee;

    // exchange rate should be updated for future
    store_state(&mut deps.storage).update(|mut prev_state| {
        prev_state.total_bond_amount += payment.amount;
        prev_state.update_exchange_rate(total_supply, requested_with_fee);
        Ok(prev_state)
    })?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // send the delegate message
    messages.push(CosmosMsg::Staking(StakingMsg::Delegate {
        validator,
        amount: payment.clone(),
    }));

    // issue the basset token for sender
    let mint_msg = Cw20HandleMsg::Mint {
        recipient: sender.clone(),
        amount: mint_amount_with_fee,
    };

    let config = read_config(&deps.storage).load()?;
    let token_address = deps.api.human_address(
        &config
            .token_contract
            .expect("the token contract must have been registered"),
    )?;

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&mint_msg)?,
        send: vec![],
    }));

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "mint"),
            log("from", sender),
            log("bonded", payment.amount),
            log("minted", mint_amount_with_fee),
        ],
        data: None,
    };
    Ok(res)
}
