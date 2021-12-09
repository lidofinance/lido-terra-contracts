// Copyright 2021 Lido
//
// Licensedicensed under the Apache License, Version 2.0 (the "License");
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

use basset::hub::{Parameters, QueryMsg};
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Coin, ContractResult, Decimal, OwnedDeps, Querier, QuerierResult,
    QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use terra_cosmwasm::{
    SwapResponse, TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};

pub const MOCK_HUB_CONTRACT_ADDR: &str = "hub";
pub const MOCK_BLUNA_REWARD_CONTRACT_ADDR: &str = "reward";
pub const MOCK_LIDO_FEE_ADDRESS: &str = "lido_fee";

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = String::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(&contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return QuerierResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                });
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        let (luna_denom, usd_denom) = ("uluna", "uusd");
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route
                    || &TerraRoute::Market == route
                    || &TerraRoute::Oracle == route
                {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: Decimal::percent(1),
                            };
                            QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom: _ } => {
                            let cap = Uint128::from(1000000u128);
                            let res = TaxCapResponse { cap };
                            QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::ExchangeRates {
                            base_denom,
                            quote_denoms,
                        } => {
                            if base_denom == luna_denom {
                                let mut exchange_rates: Vec<ExchangeRateItem> = Vec::new();
                                for quote_denom in quote_denoms {
                                    if quote_denom == "mnt" {
                                        continue;
                                    }
                                    exchange_rates.push(ExchangeRateItem {
                                        quote_denom: quote_denom.clone(),
                                        exchange_rate: Decimal::from_ratio(
                                            Uint128::from(32u64), // 1uluna = 32uusd
                                            Uint128::from(1u64),
                                        ),
                                    })
                                }
                                let res = ExchangeRatesResponse {
                                    base_denom: base_denom.to_string(),
                                    exchange_rates,
                                };
                                QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                            } else if base_denom == usd_denom {
                                let mut exchange_rates: Vec<ExchangeRateItem> = Vec::new();
                                for quote_denom in quote_denoms {
                                    if quote_denom == "mnt" {
                                        continue;
                                    }

                                    exchange_rates.push(ExchangeRateItem {
                                        quote_denom: quote_denom.clone(),
                                        exchange_rate: Decimal::from_ratio(
                                            Uint128::from(1u64), //1uusd = 0.03125uluna
                                            Uint128::from(32u64),
                                        ),
                                    })
                                }
                                let res = ExchangeRatesResponse {
                                    base_denom: base_denom.to_string(),
                                    exchange_rates,
                                };
                                QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                            } else {
                                panic!("UNSUPPORTED DENOM: {}", base_denom);
                            }
                        }
                        TerraQuery::Swap {
                            offer_coin,
                            ask_denom,
                        } => {
                            if offer_coin.denom == "usdr" && ask_denom == "uusd" {
                                QuerierResult::Ok(ContractResult::from(to_binary(&SwapResponse {
                                    receive: Coin::new(offer_coin.amount.u128() * 2, ask_denom), // 1uusd = 2usdr
                                })))
                            } else if offer_coin.denom == "uluna" && ask_denom == "uusd" {
                                QuerierResult::Ok(ContractResult::from(to_binary(&SwapResponse {
                                    receive: Coin::new(offer_coin.amount.u128() * 32, ask_denom), //1uluna = 32uusd
                                })))
                            } else if offer_coin.denom == "uusd" && ask_denom == "uluna" {
                                QuerierResult::Ok(ContractResult::from(to_binary(&SwapResponse {
                                    receive: Coin::new(offer_coin.amount.u128() / 32, ask_denom), //1uusd = 0.03125uluna
                                })))
                            } else {
                                panic!("unknown denom")
                            }
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!(
                        "UNSUPPORTED ROUTE! ROUTE: {:?}, DATA: {:?}",
                        route, query_data
                    )
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if *contract_addr == MOCK_HUB_CONTRACT_ADDR {
                    if msg == &to_binary(&QueryMsg::Parameters {}).unwrap() {
                        let params = Parameters {
                            epoch_period: 0,
                            underlying_coin_denom: "".to_string(),
                            unbonding_period: 0,
                            peg_recovery_fee: Default::default(),
                            er_threshold: Default::default(),
                            reward_denom: "".to_string(),
                            paused: None,
                        };
                        SystemResult::Ok(ContractResult::from(to_binary(&params)))
                    } else {
                        unimplemented!()
                    }
                } else {
                    unimplemented!()
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier { base }
    }
}

/// ExchangeRatesResponse is data format returned from OracleRequest::ExchangeRates query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ExchangeRatesResponse {
    pub base_denom: String,
    pub exchange_rates: Vec<ExchangeRateItem>,
}

/// ExchangeRateItem is data format returned from OracleRequest::ExchangeRates query
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ExchangeRateItem {
    pub quote_denom: String,
    pub exchange_rate: Decimal,
}
