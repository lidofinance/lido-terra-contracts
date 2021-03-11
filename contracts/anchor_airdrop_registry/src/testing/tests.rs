use crate::contract::{handle, init, query};
use crate::msg::HandleMsg::UpdateConfig;
use crate::msg::{
    ANCAirdropHandleMsg, AirdropInfoElem, AirdropInfoResponse, ConfigResponse, HandleMsg, InitMsg,
    QueryMsg,
};
use crate::msg::{MIRAirdropHandleMsg, PairHandleMsg};
use crate::state::AirdropInfo;
use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{
    from_binary, log, to_binary, Api, CosmosMsg, Env, Extern, HumanAddr, InitResponse, Querier,
    StdError, Storage, Uint128, WasmMsg,
};
use hub_querier::HandleMsg::ClaimAirdrop;

fn do_init<S: Storage, A: Api, Q: Querier>(mut deps: &mut Extern<S, A, Q>, env: Env) {
    let init_msg = InitMsg {
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
    };

    let res = init(&mut deps, env, init_msg).unwrap();

    assert_eq!(res.messages.len(), 0);
}

fn do_add_airdrop_info<S: Storage, A: Api, Q: Querier>(
    mut deps: &mut Extern<S, A, Q>,
    env: Env,
    airdrop_token: &str,
) {
    let msg = HandleMsg::AddAirdropInfo {
        airdrop_token: airdrop_token.to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 0);
}

#[test]
fn proper_init() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    let init_msg = InitMsg {
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
    };

    let res = init(&mut deps, env, init_msg).unwrap();

    assert_eq!(res.messages.len(), 0);
    assert_eq!(res, InitResponse::default());

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse = from_binary(&query(&deps, query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner,
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
        airdrop_tokens: vec![],
    };
    assert_eq!(conf, expected);
}

#[test]
fn proper_mir_claim() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "MIR");

    let msg = HandleMsg::FabricateMIRClaim {
        stage: 0,
        amount: Uint128(1000),
        proof: vec![],
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 1);

    let expected = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: HumanAddr::from("hub_contract"),
        msg: to_binary(&ClaimAirdrop {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            claim_msg: to_binary(&MIRAirdropHandleMsg::Claim {
                stage: 0,
                amount: Uint128(1000),
                proof: vec![],
            })
            .unwrap(),
            swap_msg: to_binary(&PairHandleMsg::Swap {
                belief_price: None,
                max_spread: None,
                to: Some(HumanAddr::from("reward_contract")),
            })
            .unwrap(),
        })
        .unwrap(),
        send: vec![],
    });
    assert_eq!(res.messages[0], expected);
}

#[test]
fn proper_anc_claim() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "ANC");

    let msg = HandleMsg::FabricateANCClaim {
        stage: 0,
        amount: Uint128(1000),
        proof: vec![],
    };

    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 1);

    let expected = CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: HumanAddr::from("hub_contract"),
        msg: to_binary(&ClaimAirdrop {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            claim_msg: to_binary(&ANCAirdropHandleMsg::Claim {
                stage: 0,
                amount: Uint128(1000),
                proof: vec![],
            })
            .unwrap(),
            swap_msg: to_binary(&PairHandleMsg::Swap {
                belief_price: None,
                max_spread: None,
                to: Some(HumanAddr::from("reward_contract")),
            })
            .unwrap(),
        })
        .unwrap(),
        send: vec![],
    });
    assert_eq!(res.messages[0], expected);
}

#[test]
fn proper_add_airdrop_info() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    let msg = HandleMsg::AddAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };

    // only owner can send this
    let owner = HumanAddr::from("invalid");
    let invalid_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, invalid_env, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        log("action", "add_airdrop_info"),
        log("airdrop_token", "MIR"),
    ];
    assert_eq!(res.log, expected_logs);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: Some("MIR".to_string()),
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();

    let expected = AirdropInfoResponse {
        airdrop_info: vec![AirdropInfoElem {
            airdrop_token: "MIR".to_string(),
            info: AirdropInfo {
                airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
                airdrop_contract: HumanAddr::from("airdrop_contract"),
                airdrop_swap_contract: HumanAddr::from("swap_contract"),
                swap_belief_price: None,
                swap_max_spread: None,
            },
        }],
    };
    assert_eq!(res, expected);

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse = from_binary(&query(&deps, query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner: HumanAddr::from("owner"),
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(conf, expected);

    // failed message
    let msg = HandleMsg::AddAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("new_airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };
    let res = handle(&mut deps, env, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is a token info with this MIR")
    );
}

#[test]
fn proper_remove_airdrop_info() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "MIR");

    let msg = HandleMsg::RemoveAirdropInfo {
        airdrop_token: "MIR".to_string(),
    };

    // only owner can send this
    let owner = HumanAddr::from("invalid");
    let invalid_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, invalid_env, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        log("action", "remove_airdrop_info"),
        log("airdrop_token", "MIR"),
    ];
    assert_eq!(res.log, expected_logs);

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse = from_binary(&query(&deps, query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner: HumanAddr::from("owner"),
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
        airdrop_tokens: vec![],
    };
    assert_eq!(conf, expected);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![]
        }
    );
    // failed message
    let msg = HandleMsg::UpdateAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("new_airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };
    let res = handle(&mut deps, env, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is no token info with this BUZZ")
    );
}

#[test]
fn proper_update_airdrop_info() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "MIR");

    let msg = HandleMsg::UpdateAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("new_airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };

    // only owner can send this
    let owner = HumanAddr::from("invalid");
    let invalid_env = mock_env(&owner, &[]);
    let res = handle(&mut deps, invalid_env, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::unauthorized());

    let res = handle(&mut deps, env.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        log("action", "update_airdrop_info"),
        log("airdrop_token", "MIR"),
    ];
    assert_eq!(res.log, expected_logs);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: Some("MIR".to_string()),
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();

    let expected = AirdropInfoResponse {
        airdrop_info: vec![AirdropInfoElem {
            airdrop_token: "MIR".to_string(),
            info: AirdropInfo {
                airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
                airdrop_contract: HumanAddr::from("new_airdrop_contract"),
                airdrop_swap_contract: HumanAddr::from("swap_contract"),
                swap_belief_price: None,
                swap_max_spread: None,
            },
        }],
    };
    assert_eq!(res, expected);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();

    let expected = AirdropInfo {
        airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
        airdrop_contract: HumanAddr::from("new_airdrop_contract"),
        airdrop_swap_contract: HumanAddr::from("swap_contract"),
        swap_belief_price: None,
        swap_max_spread: None,
    };
    let infos = AirdropInfoResponse {
        airdrop_info: vec![AirdropInfoElem {
            airdrop_token: "MIR".to_string(),
            info: expected,
        }],
    };

    assert_eq!(res, infos);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: Some("MIR".to_string()),
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![]
        }
    );

    // failed message
    let msg = HandleMsg::UpdateAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("new_airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };
    let res = handle(&mut deps, env, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is no token info with this BUZZ")
    );
}

#[test]
pub fn proper_update_config() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "MIR");

    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse = from_binary(&query(&deps, query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner,
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(expected, res);

    let update_conf = UpdateConfig {
        owner: Some(HumanAddr::from("new_owner")),
        hub_contract: Some(HumanAddr::from("new_hub_contract")),
        reward_contract: Some(HumanAddr::from("new_reward_contract")),
    };
    let res = handle(&mut deps, env, update_conf).unwrap();
    assert_eq!(res.messages.len(), 0);

    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse = from_binary(&query(&deps, query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner: HumanAddr::from("new_owner"),
        hub_contract: HumanAddr::from("new_hub_contract"),
        reward_contract: HumanAddr::from("new_reward_contract"),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(expected, res);
}

#[test]
fn proper_query() {
    let mut deps = mock_dependencies(20, &[]);

    let owner = HumanAddr::from("owner");
    let env = mock_env(&owner, &[]);

    do_init(&mut deps, env.clone());

    do_add_airdrop_info(&mut deps, env.clone(), "MIR");
    do_add_airdrop_info(&mut deps, env.clone(), "ANC");

    let msg = HandleMsg::AddAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: HumanAddr::from("buzz_airdrop_token_contract"),
            airdrop_contract: HumanAddr::from("buzz_airdrop_contract"),
            airdrop_swap_contract: HumanAddr::from("buzz_swap_contract"),
            swap_belief_price: None,
            swap_max_spread: None,
        },
    };
    let res = handle(&mut deps, env, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // test query config
    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse = from_binary(&query(&deps, query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner,
        hub_contract: HumanAddr::from("hub_contract"),
        reward_contract: HumanAddr::from("reward_contract"),
        airdrop_tokens: vec!["MIR".to_string(), "ANC".to_string(), "BUZZ".to_string()],
    };
    assert_eq!(expected, res);

    //test query airdrop
    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();

    let expected = AirdropInfo {
        airdrop_token_contract: HumanAddr::from("airdrop_token_contract"),
        airdrop_contract: HumanAddr::from("airdrop_contract"),
        airdrop_swap_contract: HumanAddr::from("swap_contract"),
        swap_belief_price: None,
        swap_max_spread: None,
    };
    let infos = AirdropInfoResponse {
        airdrop_info: vec![
            AirdropInfoElem {
                airdrop_token: "ANC".to_string(),
                info: expected.clone(),
            },
            AirdropInfoElem {
                airdrop_token: "BUZZ".to_string(),
                info: AirdropInfo {
                    airdrop_token_contract: HumanAddr::from("buzz_airdrop_token_contract"),
                    airdrop_contract: HumanAddr::from("buzz_airdrop_contract"),
                    airdrop_swap_contract: HumanAddr::from("buzz_swap_contract"),
                    swap_belief_price: None,
                    swap_max_spread: None,
                },
            },
            AirdropInfoElem {
                airdrop_token: "MIR".to_string(),
                info: expected.clone(),
            },
        ],
    };
    assert_eq!(res, infos);

    // test start after
    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: Some("BUZZ".to_string()),
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![AirdropInfoElem {
                airdrop_token: "MIR".to_string(),
                info: expected.clone()
            }]
        }
    );

    //test airdrop token of airdrop info query
    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: Some("MIR".to_string()),
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse = from_binary(&query(&deps, info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![AirdropInfoElem {
                airdrop_token: "MIR".to_string(),
                info: expected
            }]
        }
    );
}
