use crate::contract::slashing;
use crate::math::decimal_subtraction;
use crate::state::{
    epoch_read, get_burn_epochs, get_finished_amount, msg_status_read, parameters_read, pool_info,
    pool_info_read, read_total_amount, read_validators, remove_undelegated_wait_list, save_epoch,
    set_all_delegations, store_total_amount, store_undelegated_wait_list,
};
use cosmwasm_std::{
    coin, coins, log, to_binary, Api, BankMsg, CosmosMsg, Decimal, Env, Extern, HandleResponse,
    HumanAddr, Querier, StakingMsg, StdError, StdResult, Storage, Uint128, WasmMsg,
};
use cw20::Cw20HandleMsg;
use rand::{Rng, SeedableRng, XorShiftRng};

pub fn handle_unbond<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    amount: Uint128,
    sender: HumanAddr,
) -> StdResult<HandleResponse> {
    //read msg_status
    let msg_status = msg_status_read(&deps.storage).load()?;
    if msg_status.burn.is_some() {
        return Err(StdError::generic_err(
            "this message is temporarily deactivated",
        ));
    }

    //read params
    let params = parameters_read(&deps.storage).load()?;
    let epoch_time = params.epoch_time;
    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let mut epoch = epoch_read(&deps.storage).load()?;
    // get all amount that is gathered in a epoch.
    let mut requested_so_far = read_total_amount(&deps.storage, epoch.epoch_id)?;

    let mut messages: Vec<CosmosMsg> = vec![];

    //update pool info and calculate the new exchange rate.
    if msg_status.slashing.is_none() {
        slashing(deps, env.clone())?;
    }

    let mut exchange_rate = Decimal::zero();
    let mut amount_with_er = Uint128::zero();
    pool_info(&mut deps.storage).update(|mut pool_inf| {
        if pool_inf.exchange_rate < threshold {
            let peg_fee = decimal_subtraction(Decimal::one(), recovery_fee);
            amount_with_er = pool_inf.exchange_rate * amount * peg_fee;
        } else {
            amount_with_er = pool_inf.exchange_rate * amount;
        }
        pool_inf.total_bond_amount = (pool_inf.total_bond_amount - amount_with_er)?;
        exchange_rate = pool_inf.exchange_rate;
        Ok(pool_inf)
    })?;

    let pool = pool_info_read(&deps.storage).load()?;

    //send Burn message to token contract
    let token_address = deps.api.human_address(&pool.token_account)?;
    let burn_msg = Cw20HandleMsg::Burn { amount };
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token_address,
        msg: to_binary(&burn_msg)?,
        send: vec![],
    }));

    //compute Epoch time
    let block_time = env.block.time;

    if epoch.is_epoch_passed(block_time, epoch_time) {
        let last_epoch = epoch.epoch_id;
        epoch.compute_current_epoch(block_time, epoch_time);

        // store total amount for the next epoch to zero.
        store_total_amount(&mut deps.storage, epoch.epoch_id, Uint128::zero())?;

        let delegator = env.contract.address;

        requested_so_far += amount_with_er;

        let all_validators = read_validators(&deps.storage).unwrap();
        let block_height = env.block.height;

        // send undelegated requests
        let mut undelegated_msgs = pick_validator(
            deps,
            all_validators,
            requested_so_far,
            delegator,
            block_height,
        )?;

        //messages.append(&mut undelegated_msgs);
        messages.append(&mut undelegated_msgs);
        save_epoch(&mut deps.storage).save(&epoch)?;

        set_all_delegations(&mut deps.storage).update(|mut past| {
            past = (past - requested_so_far)?;
            Ok(past)
        })?;

        // since the sender triggered the Undelegate msg,
        // the contract store its request for the previous
        // epcoh_id
        store_undelegated_wait_list(
            &mut deps.storage,
            last_epoch,
            sender.clone(),
            amount_with_er,
        )?;
    } else {
        let luna_amount = amount_with_er;

        requested_so_far += luna_amount;

        store_undelegated_wait_list(
            &mut deps.storage,
            epoch.epoch_id,
            sender.clone(),
            luna_amount,
        )?;

        //store the claimed_so_far for the current epoch;
        store_total_amount(&mut deps.storage, epoch.epoch_id, requested_so_far)?;
    }

    let res = HandleResponse {
        messages,
        log: vec![
            log("action", "burn"),
            log("from", sender),
            log("undelegated_amount", requested_so_far),
        ],
        data: None,
    };
    Ok(res)
}

pub fn handle_withdraw_unbonded<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> StdResult<HandleResponse> {
    // read params
    let params = parameters_read(&deps.storage).load()?;
    let epoch_time = params.epoch_time;

    let sender_human = env.message.sender.clone();
    let contract_address = env.contract.address.clone();

    // check the liquidation period.
    let epoch = epoch_read(&deps.storage).load()?;
    let block_time = env.block.time;

    // get current epoch id.
    let current_epoch_id = compute_current_epoch(
        epoch.epoch_id,
        epoch.current_block_time,
        block_time,
        epoch_time,
    );

    // read params
    let params = parameters_read(&deps.storage).load()?;
    let undelegated_epoch = params.undelegated_epoch;
    let coin_denom = params.underlying_coin_denom;

    // Compute all of burn requests with epoch Id corresponding to 21 (can be changed to arbitrary value) days ago
    let epoch_id = get_past_epoch(current_epoch_id, undelegated_epoch);

    let payable_amount = get_finished_amount(&deps.storage, epoch_id, sender_human.clone())?;

    if payable_amount.is_zero() {
        return Err(StdError::generic_err(
            "Previously requested amount is not ready yet",
        ));
    }

    //remove the previous epochs for the user
    let deprecated_epochs = get_burn_epochs(&deps.storage, sender_human.clone(), epoch_id)?;
    remove_undelegated_wait_list(&mut deps.storage, deprecated_epochs, sender_human.clone())?;

    let final_amount = payable_amount;

    // return the money to the user
    let msgs = vec![BankMsg::Send {
        from_address: contract_address.clone(),
        to_address: sender_human,
        amount: coins(final_amount.u128(), &*coin_denom),
    }
    .into()];

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "finish_burn"),
            log("from", contract_address),
            log("amount", final_amount),
        ],
        data: None,
    };
    Ok(res)
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

//return the epoch-id of the 21 days ago.
pub fn get_past_epoch(current_epoch: u64, undelegated_period: u64) -> u64 {
    if current_epoch < undelegated_period {
        return 0;
    }
    current_epoch - undelegated_period
}

pub fn compute_current_epoch(
    mut epoch_id: u64,
    prev_time: u64,
    current_time: u64,
    epoch_time: u64,
) -> u64 {
    epoch_id += (current_time - prev_time) / epoch_time;
    epoch_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn proper_compute_epoch() {
        let prev_time = 1000u64;
        let current_time = 1060u64;
        let epoch_time = 30u64;
        let epoch_id = 0;
        let res = compute_current_epoch(epoch_id, prev_time, current_time, epoch_time);
        assert_eq!(res, 2u64);
    }

    #[test]
    pub fn proper_get_past_epoch() {
        //return 0
        let current_epoch = 3;
        let undelegation_period = 24;
        let past_epoch = get_past_epoch(current_epoch, undelegation_period);
        assert_eq!(past_epoch, 0);

        let current_epoch = 1024;
        let undelegation_period = 24;
        let past_epoch = get_past_epoch(current_epoch, undelegation_period);
        assert_eq!(past_epoch, 1000)
    }
}
