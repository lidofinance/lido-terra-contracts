// Copyright 2021 Anchor Protocol. Modified by Lido
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::contract::{execute, instantiate, query};
use basset::airdrop::{
    AirdropInfoElem, AirdropInfoResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg,
};

use super::mock_querier::mock_dependencies;
use basset::airdrop::AirdropInfo;
use basset::airdrop::ExecuteMsg::UpdateConfig;
use cosmwasm_std::testing::{mock_env, mock_info};

use cosmwasm_std::{attr, from_binary, DepsMut, Env, MessageInfo, Response, StdError};

fn do_init(deps: DepsMut, env: Env, info: MessageInfo) {
    let init_msg = InstantiateMsg {
        hub_contract: "hub_contract".to_string(),
        reward_contract: "reward_contract".to_string(),
    };

    let res = instantiate(deps, env, info, init_msg).unwrap();

    assert_eq!(res.messages.len(), 0);
}

fn do_add_airdrop_info(deps: DepsMut, env: Env, info: MessageInfo, airdrop_token: &str) {
    let msg = ExecuteMsg::AddAirdropInfo {
        airdrop_token: airdrop_token.to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "airdrop_contract".to_string(),
        },
    };
    let res = execute(deps, env, info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);
}

#[test]
fn proper_init() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    let init_msg = InstantiateMsg {
        hub_contract: "hub_contract".to_string(),
        reward_contract: "reward_contract".to_string(),
    };

    let res = instantiate(deps.as_mut(), mock_env(), info, init_msg).unwrap();

    assert_eq!(res.messages.len(), 0);
    assert_eq!(res, Response::default());

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner: "owner".to_string(),
        hub_contract: "hub_contract".to_string(),
        airdrop_tokens: vec![],
    };
    assert_eq!(conf, expected);
}

#[test]
fn proper_add_airdrop_info() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    do_init(deps.as_mut(), mock_env(), info.clone());

    let msg = ExecuteMsg::AddAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "airdrop_contract".to_string(),
        },
    };

    // only owner can send this
    let owner = "invalid";
    let invalid_info = mock_info(owner, &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        attr("action", "add_airdrop_info"),
        attr("airdrop_token", "MIR"),
    ];
    assert_eq!(res.attributes, expected_logs);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: Some("MIR".to_string()),
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();

    let expected = AirdropInfoResponse {
        airdrop_info: vec![AirdropInfoElem {
            airdrop_token: "MIR".to_string(),
            info: AirdropInfo {
                airdrop_token_contract: "airdrop_token_contract".to_string(),
                airdrop_contract: "airdrop_contract".to_string(),
            },
        }],
    };
    assert_eq!(res, expected);

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner: "owner".to_string(),
        hub_contract: "hub_contract".to_string(),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(conf, expected);

    // failed message
    let msg = ExecuteMsg::AddAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "new_airdrop_contract".to_string(),
        },
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is a token info with this MIR")
    );
}

#[test]
fn proper_remove_airdrop_info() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    do_init(deps.as_mut(), mock_env(), info.clone());

    do_add_airdrop_info(deps.as_mut(), mock_env(), info.clone(), "MIR");

    let msg = ExecuteMsg::RemoveAirdropInfo {
        airdrop_token: "MIR".to_string(),
    };

    // only owner can send this
    let invalid_info = mock_info("invalid", &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        attr("action", "remove_airdrop_info"),
        attr("airdrop_token", "MIR"),
    ];
    assert_eq!(res.attributes, expected_logs);

    let query_conf = QueryMsg::Config {};
    let conf: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_conf).unwrap()).unwrap();

    let expected = ConfigResponse {
        owner: "owner".to_string(),
        hub_contract: "hub_contract".to_string(),
        airdrop_tokens: vec![],
    };
    assert_eq!(conf, expected);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![]
        }
    );
    // failed message
    let msg = ExecuteMsg::UpdateAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "new_airdrop_contract".to_string(),
        },
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is no token info with this BUZZ")
    );
}

#[test]
fn proper_update_airdrop_info() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    do_init(deps.as_mut(), mock_env(), info.clone());

    do_add_airdrop_info(deps.as_mut(), mock_env(), info.clone(), "MIR");

    let msg = ExecuteMsg::UpdateAirdropInfo {
        airdrop_token: "MIR".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "new_airdrop_contract".to_string(),
        },
    };

    // only owner can send this
    let invalid_info = mock_info("invalid", &[]);
    let res = execute(deps.as_mut(), mock_env(), invalid_info, msg.clone());
    assert_eq!(res.unwrap_err(), StdError::generic_err("unauthorized"));

    let res = execute(deps.as_mut(), mock_env(), info.clone(), msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    let expected_logs = vec![
        attr("action", "update_airdrop_info"),
        attr("airdrop_token", "MIR"),
    ];
    assert_eq!(res.attributes, expected_logs);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: Some("MIR".to_string()),
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();

    let expected = AirdropInfoResponse {
        airdrop_info: vec![AirdropInfoElem {
            airdrop_token: "MIR".to_string(),
            info: AirdropInfo {
                airdrop_token_contract: "airdrop_token_contract".to_string(),
                airdrop_contract: "new_airdrop_contract".to_string(),
            },
        }],
    };
    assert_eq!(res, expected);

    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();

    let expected = AirdropInfo {
        airdrop_token_contract: "airdrop_token_contract".to_string(),
        airdrop_contract: "new_airdrop_contract".to_string(),
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
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();
    assert_eq!(
        res,
        AirdropInfoResponse {
            airdrop_info: vec![]
        }
    );

    // failed message
    let msg = ExecuteMsg::UpdateAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "airdrop_token_contract".to_string(),
            airdrop_contract: "new_airdrop_contract".to_string(),
        },
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap_err();
    assert_eq!(
        res,
        StdError::generic_err("There is no token info with this BUZZ")
    );
}

#[test]
pub fn proper_update_config() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    do_init(deps.as_mut(), mock_env(), info.clone());

    do_add_airdrop_info(deps.as_mut(), mock_env(), info.clone(), "MIR");

    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner: "owner".to_string(),
        hub_contract: "hub_contract".to_string(),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(expected, res);

    let update_conf = UpdateConfig {
        owner: Some("new_owner".to_string()),
        hub_contract: Some("new_hub_contract".to_string()),
    };
    let res = execute(deps.as_mut(), mock_env(), info, update_conf).unwrap();
    assert_eq!(res.messages.len(), 0);

    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner: "new_owner".to_string(),
        hub_contract: "new_hub_contract".to_string(),
        airdrop_tokens: vec!["MIR".to_string()],
    };
    assert_eq!(expected, res);
}

#[test]
fn proper_query() {
    let mut deps = mock_dependencies(&[]);

    let info = mock_info("owner", &[]);

    do_init(deps.as_mut(), mock_env(), info.clone());

    do_add_airdrop_info(deps.as_mut(), mock_env(), info.clone(), "MIR");
    do_add_airdrop_info(deps.as_mut(), mock_env(), info.clone(), "ANC");

    let msg = ExecuteMsg::AddAirdropInfo {
        airdrop_token: "BUZZ".to_string(),
        airdrop_info: AirdropInfo {
            airdrop_token_contract: "buzz_airdrop_token_contract".to_string(),
            airdrop_contract: "buzz_airdrop_contract".to_string(),
        },
    };
    let res = execute(deps.as_mut(), mock_env(), info, msg).unwrap();
    assert_eq!(res.messages.len(), 0);

    // test query config
    let query_update_config = QueryMsg::Config {};
    let res: ConfigResponse =
        from_binary(&query(deps.as_ref(), mock_env(), query_update_config).unwrap()).unwrap();
    let expected = ConfigResponse {
        owner: "owner".to_string(),
        hub_contract: "hub_contract".to_string(),
        airdrop_tokens: vec!["MIR".to_string(), "ANC".to_string(), "BUZZ".to_string()],
    };
    assert_eq!(expected, res);

    //test query airdrop
    let info_query = QueryMsg::AirdropInfo {
        airdrop_token: None,
        start_after: None,
        limit: None,
    };
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();

    let expected = AirdropInfo {
        airdrop_token_contract: "airdrop_token_contract".to_string(),
        airdrop_contract: "airdrop_contract".to_string(),
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
                    airdrop_token_contract: "buzz_airdrop_token_contract".to_string(),
                    airdrop_contract: "buzz_airdrop_contract".to_string(),
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
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();
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
    let res: AirdropInfoResponse =
        from_binary(&query(deps.as_ref(), mock_env(), info_query).unwrap()).unwrap();
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
