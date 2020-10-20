use crate::msg::HandleMsg;
use cosmwasm_std::{
    coins, log, Api, BankMsg, Env, Extern, HandleResponse, HumanAddr, Querier, StdResult, Storage,
    Uint128,
};

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> StdResult<HandleResponse> {
    match msg {
        HandleMsg::SendReward { receiver, amount } => handle_send(deps, env, receiver, amount),
    }
}

pub fn handle_send<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    env: Env,
    receiver: HumanAddr,
    amount: Uint128,
) -> StdResult<HandleResponse> {
    //TODO: check whether the gov contract has sent this
    //TODO: use swap message
    let contr_addr = env.contract.address;
    let msgs = vec![BankMsg::Send {
        from_address: contr_addr.clone(),
        to_address: receiver,
        amount: coins(Uint128::u128(&amount), "uluna"),
    }
    .into()];

    let res = HandleResponse {
        messages: msgs,
        log: vec![
            log("action", "send_reward"),
            log("from", contr_addr),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}
