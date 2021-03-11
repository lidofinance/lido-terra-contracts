use crate::contract::{query_total_issued, slashing};
use crate::state::{
    get_finished_amount, get_unbond_batches, read_config, read_current_batch, read_parameters,
    read_state, read_unbond_history, remove_unbond_wait_list, store_current_batch, store_state,
    store_unbond_history, store_unbond_wait_list, UnbondHistory,
};
use cosmwasm_std::{
    coin, coins, log, to_binary, Api, BankMsg, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, Querier, StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;
use rand::{Rng, SeedableRng, XorShiftRng};
use signed_integer::SignedInt;

/// This message must be call by receive_cw20
/// This message will undelegate coin and burn basset token
pub(crate) fn handle_unbond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
) -> StdResult<HandleResponse> {
    // Read params
    let params = read_parameters(&deps.storage).load()?;
    let epoch_period = params.epoch_period;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let mut current_batch = read_current_batch(&deps.storage).load()?;

    // Check slashing, update state, and calculate the new exchange rate.
    slashing(deps, env.clone())?;

    let mut state = read_state(&deps.storage).load()?;

    let mut total_supply = query_total_issued(&deps).unwrap_or_default();

    // Collect all the requests within a epoch period
    // Apply peg recovery fee
    let amount_with_fee: Uint128;
    if state.exchange_rate < threshold {
        let max_peg_fee = amount * recovery_fee;
        let required_peg_fee =
            ((total_supply + current_batch.requested_with_fee) - state.total_bond_amount)?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        amount_with_fee = (amount - peg_fee)?;
    } else {
        amount_with_fee = amount;
    }
    current_batch.requested_with_fee += amount_with_fee;

    store_unbond_wait_list(
        &mut deps.storage,
        current_batch.id,
        sender.clone(),
        amount_with_fee,
    )?;

    total_supply =
        (total_supply - amount).expect("the requested can not be more than the total supply");

    // Update exchange rate
    state.update_exchange_rate(total_supply, current_batch.requested_with_fee);

    let current_time = env.block.time;
    let passed_time = current_time - state.last_unbonded_time;

    let mut messages: Vec<CosmosMsg> = vec![];

    // If the epoch period is passed, the undelegate message would be sent.
    if passed_time > epoch_period {
        // Apply the current exchange rate.
        let undelegation_amount = current_batch.requested_with_fee * state.exchange_rate;

        // the contract must stop if
        if undelegation_amount == Uint128(1) {
            return Err(StdError::generic_err(
                "Burn amount must be greater than 1 ubluna",
            ));
        }

        let delegator = env.contract.address;

        let block_height = env.block.height;

        // Send undelegated requests to possibly more than one validators
        let mut undelegated_msgs =
            pick_validator(deps, undelegation_amount, delegator, block_height)?;

        messages.append(&mut undelegated_msgs);

        state.total_bond_amount = (state.total_bond_amount - undelegation_amount)
            .expect("undelegation amount can not be more than stored total bonded amount");

        // Store history for withdraw unbonded
        let history = UnbondHistory {
            batch_id: current_batch.id,
            time: env.block.time,
            amount: current_batch.requested_with_fee,
            applied_exchange_rate: state.exchange_rate,
            withdraw_rate: state.exchange_rate,
            released: false,
        };
        store_unbond_history(&mut deps.storage, current_batch.id, history)?;
        // batch info must be updated to new batch
        current_batch.id += 1;
        current_batch.requested_with_fee = Uint128::zero();

        // state.last_unbonded_time must be updated to the current block time
        state.last_unbonded_time = env.block.time;
    }

    // Store the new requested_with_fee or id in the current batch
    store_current_batch(&mut deps.storage).save(&current_batch)?;

    // Store state's new exchange rate
    store_state(&mut deps.storage).save(&state)?;

    // Send Burn message to token contract
    let config = read_config(&deps.storage).load()?;
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
            log("burnt_amount", amount),
            log("unbonded_amount", amount_with_fee),
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
    let params = read_parameters(&deps.storage).load()?;
    let unbonding_period = params.unbonding_period;
    let coin_denom = params.underlying_coin_denom;

    let historical_time = env.block.time - unbonding_period;

    // query hub balance for process withdraw rate.
    let hub_balance = deps
        .querier
        .query_balance(&env.contract.address, &*coin_denom)?
        .amount;

    // calculate withdraw rate for user requests
    process_withdraw_rate(deps, historical_time, hub_balance)?;

    let withdraw_amount = get_finished_amount(&deps.storage, sender_human.clone()).unwrap();

    if withdraw_amount.is_zero() {
        return Err(StdError::generic_err(format!(
            "No withdrawable {} assets are available yet",
            coin_denom
        )));
    }

    // remove the previous batches for the user
    let deprecated_batches = get_unbond_batches(&deps.storage, sender_human.clone())?;
    remove_unbond_wait_list(&mut deps.storage, deprecated_batches, sender_human.clone())?;

    // Update previous balance used for calculation in next Luna batch release
    let prev_balance = (hub_balance - withdraw_amount)?;
    store_state(&mut deps.storage).update(|mut last_state| {
        last_state.prev_hub_balance = prev_balance;
        Ok(last_state)
    })?;

    // Send the money to the user
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

/// This is designed for an accurate unbonded amount calculation.
/// Execute while processing withdraw_unbonded
fn process_withdraw_rate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    historical_time: u64,
    hub_balance: Uint128,
) -> StdResult<()> {
    // balance change of the hub contract must be checked.
    let mut total_unbonded_amount = Uint128::zero();

    let mut state = read_state(&deps.storage).load()?;

    let balance_change = SignedInt::from_subtraction(hub_balance, state.prev_hub_balance);
    state.actual_unbonded_amount += balance_change.0;

    let last_processed_batch = state.last_processed_batch;
    let mut batch_count: u64 = 0;

    // Iterate over unbonded histories that have been processed
    // to calculate newly added unbonded amount
    let mut i = last_processed_batch + 1;
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
        // Use signed integer in case of some rogue transfers.
        let slashed_amount =
            SignedInt::from_subtraction(total_unbonded_amount, state.actual_unbonded_amount);

        // Iterate again to calculate the withdraw rate for each unprocessed history
        let mut iterator = last_processed_batch + 1;
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

            // the slashed amount for each batch must be proportional to the unbonded amount of batch
            let batch_slashing_weight =
                Decimal::from_ratio(unbonded_amount_of_batch, total_unbonded_amount);

            let mut slashed_amount_of_batch = batch_slashing_weight * slashed_amount.0;
            let actual_unbonded_amount_of_batch: Uint128;

            // If slashed amount is negative, there should be summation instead of subtraction.
            if slashed_amount.1 {
                slashed_amount_of_batch = (slashed_amount_of_batch - Uint128(1))?;
                actual_unbonded_amount_of_batch =
                    unbonded_amount_of_batch + slashed_amount_of_batch;
            } else {
                if slashed_amount.0.u128() != 0u128 {
                    slashed_amount_of_batch += Uint128(1);
                }
                actual_unbonded_amount_of_batch =
                    SignedInt::from_subtraction(unbonded_amount_of_batch, slashed_amount_of_batch)
                        .0;
            }
            // Calculate the new withdraw rate
            let new_withdraw_rate =
                Decimal::from_ratio(actual_unbonded_amount_of_batch, burnt_amount_of_batch);

            let mut history_for_i = history;
            // store the history and mark it as released
            history_for_i.withdraw_rate = new_withdraw_rate;
            history_for_i.released = true;
            store_unbond_history(&mut deps.storage, iterator, history_for_i)?;
            state.last_processed_batch = iterator;
            iterator += 1;
        }
    }
    // Store state.actual_unbonded_amount for future new batches release
    state.actual_unbonded_amount = Uint128::zero();
    store_state(&mut deps.storage).save(&state)?;

    Ok(())
}

fn pick_validator<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    claim: Uint128,
    delegator: HumanAddr,
    block_height: u64,
) -> StdResult<Vec<CosmosMsg>> {
    //read params
    let params = read_parameters(&deps.storage).load()?;
    let coin_denom = params.underlying_coin_denom;

    let mut messages: Vec<CosmosMsg> = vec![];
    let mut claimed = claim;

    let all_delegations = deps
        .querier
        .query_all_delegations(delegator)
        .expect("There must be at least one delegation");
    // pick a random validator
    // if it does not have requested amount, undelegate all it has
    // and pick another random validator
    let mut iteration_index = 0;
    let mut deletable_delegations = all_delegations;
    while claimed.0 > 0 {
        let mut rng = XorShiftRng::seed_from_u64(block_height + iteration_index);
        let random_index = rng.gen_range(0, deletable_delegations.len());
        let delegation = deletable_delegations.remove(random_index);
        let val = delegation.amount.amount;
        let undelegated_amount: Uint128;
        if val.0 > claimed.0 {
            undelegated_amount = claimed;
            claimed = Uint128::zero();
        } else {
            undelegated_amount = val;
            claimed = (claimed - val)?;
        }
        if undelegated_amount.0 > 0 {
            let msgs: CosmosMsg = CosmosMsg::Staking(StakingMsg::Undelegate {
                validator: delegation.validator,
                amount: coin(undelegated_amount.0, &*coin_denom),
            });
            messages.push(msgs);
        }
        iteration_index += 1;
    }
    Ok(messages)
}
