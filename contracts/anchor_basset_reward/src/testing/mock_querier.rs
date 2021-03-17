use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Api, Coin, Decimal, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use hub_querier::Config;
use std::str::FromStr;
use terra_cosmwasm::{
    ExchangeRateItem, ExchangeRatesResponse, TaxCapResponse, TaxRateResponse, TerraQuery,
    TerraQueryWrapper, TerraRoute,
};

pub const MOCK_HUB_CONTRACT_ADDR: &str = "hub";
pub const MOCK_REWARD_CONTRACT_ADDR: &str = "reward";
pub const MOCK_TOKEN_CONTRACT_ADDR: &str = "token";

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
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                if &TerraRoute::Treasury == route {
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
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else if &TerraRoute::Oracle == route {
                    match query_data {
                        TerraQuery::ExchangeRates {
                            base_denom,
                            quote_denoms,
                        } => {
                            if quote_denoms.iter().any(|item| item == &"mnt".to_string()) {
                                return Err(SystemError::Unknown {});
                            }
                            Ok(to_binary(&ExchangeRatesResponse {
                                base_denom: base_denom.to_string(),
                                exchange_rates: vec![ExchangeRateItem {
                                    quote_denom: quote_denoms[0].to_string(),
                                    exchange_rate: Decimal::from_str("22.1").unwrap(),
                                }],
                            }))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                if *contract_addr == HumanAddr::from(MOCK_HUB_CONTRACT_ADDR) {
                    let prefix_config = to_length_prefixed(b"config").to_vec();
                    let api: MockApi = MockApi::new(self.canonical_length);
                    if key.as_slice().to_vec() == prefix_config {
                        let config = Config {
                            creator: api.canonical_address(&HumanAddr::from("owner1")).unwrap(),
                            reward_contract: Some(
                                api.canonical_address(&HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR))
                                    .unwrap(),
                            ),
                            token_contract: Some(
                                api.canonical_address(&HumanAddr::from(MOCK_TOKEN_CONTRACT_ADDR))
                                    .unwrap(),
                            ),
                            airdrop_registry_contract: Some(
                                api.canonical_address(&HumanAddr::from("airdrop")).unwrap(),
                            ),
                        };
                        Ok(to_binary(&to_binary(&config).unwrap()))
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
