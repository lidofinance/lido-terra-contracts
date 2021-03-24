use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Coin, Decimal, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute, SwapResponse};
use crate::contract::{LUNA_DENOM, USD_DENOM};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MOCK_HUB_CONTRACT_ADDR: &str = "hub";
// pub const MOCK_REWARD_CONTRACT_ADDR: &str = "reward";
// pub const MOCK_TOKEN_CONTRACT_ADDR: &str = "token";

pub fn mock_dependencies(
    canonical_length: usize,
    contract_balance: &[Coin],
) -> Extern<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(
        MockQuerier::new(&[(&contract_addr, contract_balance)]),
        canonical_length,
    );

    Extern {
        storage: MockStorage::default(),
        api: MockApi::new(canonical_length),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    canonical_length: usize,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return Err(SystemError::InvalidRequest {
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
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route || &TerraRoute::Market == route || &TerraRoute::Oracle == route {
                    match query_data {
                        TerraQuery::TaxRate {} => {
                            let res = TaxRateResponse {
                                rate: Decimal::percent(1),
                            };
                            Ok(to_binary(&res))
                        }
                        TerraQuery::TaxCap { denom: _ } => {
                            let cap = Uint128(1000000u128);
                            let res = TaxCapResponse { cap };
                            Ok(to_binary(&res))
                        }
                        TerraQuery::ExchangeRates { base_denom, quote_denoms } => {
                            if base_denom == LUNA_DENOM {
                                let mut exchange_rates: Vec<ExchangeRateItem> = Vec::new();
                                for quote_denom in quote_denoms {
                                    exchange_rates.push(ExchangeRateItem {
                                        quote_denom: quote_denom.clone(),
                                        exchange_rate: Decimal::from_ratio(Uint128(1), Uint128(1)),
                                    })
                                }
                                let res = ExchangeRatesResponse {
                                    base_denom: base_denom.to_string(),
                                    exchange_rates,
                                };
                                Ok(to_binary(&res))
                            } else if base_denom == USD_DENOM {
                                let mut exchange_rates: Vec<ExchangeRateItem> = Vec::new();
                                for quote_denom in quote_denoms {
                                    exchange_rates.push(ExchangeRateItem {
                                        quote_denom: quote_denom.clone(),
                                        exchange_rate: Decimal::from_ratio(Uint128(1), Uint128(1)),
                                    })
                                }
                                let res = ExchangeRatesResponse {
                                    base_denom: base_denom.to_string(),
                                    exchange_rates,
                                };
                                Ok(to_binary(&res))
                            } else {
                                panic!("UNSUPPORTED DENOM: {}", base_denom);
                            }
                        }
                        TerraQuery::Swap { offer_coin, ask_denom } => {
                            Ok(to_binary(&SwapResponse {
                                receive: Coin::new(offer_coin.amount.u128(), ask_denom),
                            }))
                        }
                    }
                } else {
                    panic!("UNSUPPORTED ROUTE! ROUTE: {:?}, DATA: {:?}", route, query_data)
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                if *contract_addr == HumanAddr::from(MOCK_HUB_CONTRACT_ADDR) {
                    let prefix_config = to_length_prefixed(b"config").to_vec();
                    if key.as_slice().to_vec() == prefix_config {
                        unimplemented!()
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
    pub fn new(base: MockQuerier<TerraQueryWrapper>, canonical_length: usize) -> Self {
        WasmMockQuerier {
            base,
            canonical_length,
        }
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
