use crate::contract::{query_total_issued, slashing};
use crate::math::decimal_subtraction;
use crate::state::{
    config_read, get_finished_amount, get_unbond_batches, msg_status_read, parameters_read,
    read_current_batch, read_state, read_unbond_history, read_validators,
    remove_undelegated_wait_list, store_current_batch, store_state, store_unbond_history,
    store_undelegated_wait_list, UnbondHistory,
};
use cosmwasm_std::{
    coin, coins, log, to_binary, Api, BankMsg, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, Querier, StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;
use rand::{Rng, SeedableRng, XorShiftRng};
use unsigned_integer::UnsignedInt;

pub fn handle_unbond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
) -> StdResult<HandleResponse> {
    //read msg_status
    let msg_status = msg_status_read(&deps.storage).load()?;
    if msg_status.unbond.is_some() {
        return Err(StdError::generic_err(
            "this message is temporarily deactivated",
        ));
    }

    //read params
    let params = parameters_read(&deps.storage).load()?;
    let epoch_period = params.epoch_period;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let mut current_batch = read_current_batch(&deps.storage).load()?;

    //update pool info and calculate the new exchange rate.
    if msg_status.slashing.is_none() {
        slashing(deps, env.clone())?;
    }

    let mut state = read_state(&deps.storage).load()?;

    let amount_with_fee: Uint128;
    if state.exchange_rate < threshold {
        let peg_fee = decimal_subtraction(Decimal::one(), recovery_fee);
        amount_with_fee = amount * peg_fee;
    } else {
        amount_with_fee = amount;
    }
    current_batch.requested_with_fee += amount_with_fee;

    store_undelegated_wait_list(
        &mut deps.storage,
        current_batch.id,
        sender.clone(),
        amount_with_fee,
    )?;

    let mut total_supply = query_total_issued(&deps).unwrap_or_default();
    total_supply =
        (total_supply - amount).expect("the requested must not be more than the total supply");

    //update exchange rate
    state.update_exchange_rate(total_supply, current_batch.requested_with_fee);

    let current_time = env.block.time;
    let passed_time = current_time - state.last_unbonded_time;

    let mut messages: Vec<CosmosMsg> = vec![];

    if passed_time > epoch_period {
        let undelegation_amount = current_batch.requested_with_fee * state.exchange_rate;

        let delegator = env.contract.address;

        let all_validators = read_validators(&deps.storage).unwrap();
        let block_height = env.block.height;

        // send undelegated requests
        let mut undelegated_msgs = pick_validator(
            deps,
            all_validators,
            undelegation_amount,
            delegator,
            block_height,
        )?;

        //messages.append(&mut undelegated_msgs);
        messages.append(&mut undelegated_msgs);

        state.total_bond_amount = (state.total_bond_amount - undelegation_amount)
            .expect("undelegation amount must not be more than stored total bonded amount");

        let history = UnbondHistory {
            time: env.block.time,
            amount: current_batch.requested_with_fee,
            withdraw_rate: state.exchange_rate,
            released: false,
        };
        store_unbond_history(&mut deps.storage, current_batch.id, history)?;
        current_batch.id += 1;
        current_batch.requested_with_fee = Uint128::zero();
        state.last_unbonded_time = env.block.time;
    }

    store_current_batch(&mut deps.storage).save(&current_batch)?;

    store_state(&mut deps.storage).save(&state)?;

    //send Burn message to token contract
    let config = config_read(&deps.storage).load()?;
    let token_address = deps.api.human_address(
        &config
            .token_contract
            .expect("the token contract must have been registered"),
    )?;

    let burn_msg = Cw20HandleMsg::Burn { amount };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&burn_msg)?,
        send: vec![],
    }));

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "burn"),
            log("from", sender),
            log("undelegated_amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_withdraw_unbonded<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    let sender_human = env.message.sender.clone();
    let contract_address = env.contract.address.clone();

    // read params
    let params = parameters_read(&deps.storage).load()?;
    let unbonding_period = params.unbonding_period;
    let coin_denom = params.underlying_coin_denom;

    let historical_time = env.block.time - unbonding_period;

    let hub_balance = deps
        .querier
        .query_balance(&env.contract.address, &*coin_denom)?
        .amount;

    process_withdraw_rate(deps, historical_time, hub_balance)?;

    let withdraw_amount = get_finished_amount(&deps.storage, sender_human.clone()).unwrap();

    if withdraw_amount.is_zero() {
        return Err(StdError::generic_err(
            "Previously requested amount is not ready yet",
        ));
    }

    //remove the previous batches for the user
    let deprecated_batches = get_unbond_batches(&deps.storage, sender_human.clone())?;
    remove_undelegated_wait_list(&mut deps.storage, deprecated_batches, sender_human.clone())?;

    let prev_balance = (hub_balance - withdraw_amount)?;
    store_state(&mut deps.storage).update(|mut last_state| {
        last_state.prev_hub_balance = prev_balance;
        Ok(last_state)
    })?;

    // return the money to the user
    let msgs = vec![BankMsg::Send {
        from_address: contract_address.clone(),
        to_address: sender_human,
        amount: coins(withdraw_amount.u128(), &*coin_denom),
    }
    .into()];

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "finish_burn"),
            log("from", contract_address),
            log("amount", withdraw_amount),
        ],
        data: None,
    };
    Ok(res)
}

fn process_withdraw_rate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    historical_time: u64,
    hub_balance: Uint128,
) -> StdResult<()> {
    //slashing related operations
    let mut total_unbonded_amount = Uint128::zero();

    let mut state = read_state(&deps.storage).load()?;

    let balance_change = UnsignedInt::from_subtraction(hub_balance, state.prev_hub_balance);
    state.actual_unbonded_amount += balance_change.0;

    let last_processed_batch = state.last_processed_batch;
    let mut batch_count: u64 = 0;

    let mut i = last_processed_batch;
    loop {
        let history: UnbondHistory;
        match read_unbond_history(&deps.storage, i) {
            Ok(h) => {
                if h.time > historical_time {
                    break;
                }
                if !h.released {
                    history = h.clone();
                } else {
                    break;
                }
            }
            Err(_) => break,
        }
        let burnt_amount = history.amount;
        let historical_rate = history.withdraw_rate;
        let unbonded_amount = burnt_amount * historical_rate;
        total_unbonded_amount += unbonded_amount;
        batch_count += 1;
        i += 1;
    }

    if batch_count >= 1 {
        let slashed_amount_per_batch: Uint128;
        let slashed_amount =
            UnsignedInt::from_subtraction(total_unbonded_amount, state.actual_unbonded_amount);
        if batch_count == 0 {
            slashed_amount_per_batch = slashed_amount.0
        } else {
            slashed_amount_per_batch = Uint128(slashed_amount.0.u128() / u128::from(batch_count));
        }

        let mut iterator = last_processed_batch;
        loop {
            let history: UnbondHistory;
            match read_unbond_history(&deps.storage, iterator) {
                Ok(h) => {
                    if h.time > historical_time {
                        break;
                    }
                    if !h.released {
                        history = h
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
            let burnt_amount_of_batch = history.amount;
            let historical_rate_of_batch = history.withdraw_rate;
            let unbonded_amount_of_batch = burnt_amount_of_batch * historical_rate_of_batch;
            let actual_unbonded_amount_of_batch: Uint128;
            if slashed_amount.1 {
                actual_unbonded_amount_of_batch =
                    unbonded_amount_of_batch + slashed_amount_per_batch
            } else {
                actual_unbonded_amount_of_batch = UnsignedInt::from_subtraction(
                    unbonded_amount_of_batch,
                    slashed_amount_per_batch,
                )
                .0;
            }
            let new_withdraw_rate =
                Decimal::from_ratio(actual_unbonded_amount_of_batch, burnt_amount_of_batch);
            let mut history_for_i = read_unbond_history(&deps.storage, iterator)
                .expect("the existence of history is checked before");
            history_for_i.withdraw_rate = new_withdraw_rate;
            history_for_i.released = true;
            store_unbond_history(&mut deps.storage, iterator, history_for_i)?;
            state.last_processed_batch = iterator;
            iterator += 1;
        }
    }
    state.actual_unbonded_amount = Uint128::zero();
    store_state(&mut deps.storage).save(&state)?;

    Ok(())
}

fn pick_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    validators: Vec<HumanAddr>,
    claim: Uint128,
    delegator: HumanAddr,
    block_height: u64,
) -> StdResult<Vec<CosmosMsg>> {
    //read params
    let params = parameters_read(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut claimed = claim;
    let mut rng = XorShiftRng::seed_from_u64(block_height);

    while claimed.0 > 0 {
        let random_index = rng.gen_range(0, validators.len());
        let validator: HumanAddr = HumanAddr::from(validators.get(random_index).unwrap());
        let val = deps
            .querier
            .query_delegation(delegator.clone(), validator.clone())
            .unwrap()
            .unwrap()
            .amount
            .amount;
        let undelegated_amount: Uint128;
        if val.0 > claimed.0 {
            undelegated_amount = claimed;
            claimed = Uint128::zero();
        } else {
            undelegated_amount = val;
            claimed = Uint128(claimed.0 - val.0);
        }
        let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
            validator,
            amount: coin(undelegated_amount.0, &*coin_denom),
        });
        messages.push(msgs);
    }
    Ok(messages)
}
