use crate::contract::{query_total_issued, slashing};
use crate::math::{decimal_division, decimal_subtraction};
use crate::state::{
    config_read, is_valid_validator, msg_status_read, parameters_read, read_current_batch,
    read_state, store_state,
};
use cosmwasm_std::{
    log, to_binary, Api, CosmosMsg, Decimal, Env, Extern, HandleResponse, HumanAddr, Querier,
    StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;

pub fn handle_bond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    validator: HumanAddr,
) -> StdResult<HandleResponse> {
    // check the validator
    let is_valid = is_valid_validator(&deps.storage, validator.clone())?;
    if !is_valid {
        return Err(StdError::generic_err("Unsupported validator"));
    }

    // read params
    let params = parameters_read(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    // read current batch
    let current_batch = read_current_batch(&deps.storage).load()?;
    let requested_with_fee = current_batch.requested_with_fee;

    // check whether the account has sent the native coin in advance.
    let payment = env
        .message
        .sent_funds
        .iter()
        .find(|x| x.denom == coin_denom && x.amount > Uint128::zero())
        .ok_or_else(|| StdError::generic_err(format!("No {} tokens sent", coin_denom)))?;

    // msg status should be read to see whether the slashing is active or not
    let msg_status = msg_status_read(&deps.storage).load()?;

    // update the exchange rate
    if msg_status.slashing.is_none() && slashing(deps, env.clone()).is_ok() {
        slashing(deps, env.clone())?;
    }

    let mut state = read_state(&deps.storage).load()?;
    let sender = env.message.sender.clone();

    // peg recovery fee should be considered
    let mint_amount = decimal_division(payment.amount, state.exchange_rate);
    let mut mint_amount_with_fee = mint_amount;
    if state.exchange_rate < threshold {
        let peg_fee = decimal_subtraction(Decimal::one(), recovery_fee);
        mint_amount_with_fee = mint_amount * peg_fee;
    }

    // total supply should be updated for exchange rate calculation.
    let mut total_supply = query_total_issued(&deps).unwrap_or_default();
    total_supply += mint_amount_with_fee;

    // update state and state exchange rate
    state.total_bond_amount += payment.amount;
    store_state(&mut deps.storage).update(|mut state| {
        state.total_bond_amount += payment.amount;
        state.update_exchange_rate(total_supply, requested_with_fee);
        Ok(state)
    })?;

    let mut messages: Vec<CosmosMsg> = vec![];

    // send the delegate message for the specified validator with the send amount.
    messages.push(CosmosMsg::Staking(StakingMsg::Delegate {
        validator,
        amount: payment.clone(),
    }));

    // issue the bluna token for sender
    let mint_msg = Cw20HandleMsg::Mint {
        recipient: sender.clone(),
        amount: mint_amount_with_fee,
    };

    let config = config_read(&deps.storage).load()?;
    let token_address = deps.api.human_address(
        &config
            .token_contract
            .expect("the reward contract must have been registered"),
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
