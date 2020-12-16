use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Api, Coin, Decimal, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError, Uint128, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use hub_courier::PoolInfo;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};

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
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                if *contract_addr == HumanAddr::from(MOCK_HUB_CONTRACT_ADDR) {
                    let prefix_pool = to_length_prefixed(b"pool_info").to_vec();
                    let api: MockApi = MockApi::new(self.canonical_length);
                    if key.as_slice().to_vec() == prefix_pool {
                        let pool = PoolInfo {
                            exchange_rate: Default::default(),
                            total_bond_amount: Default::default(),
                            last_index_modification: 0,
                            reward_account: api
                                .canonical_address(&HumanAddr::from(MOCK_REWARD_CONTRACT_ADDR))
                                .unwrap(),
                            is_reward_exist: true,
                            is_token_exist: true,
                            token_account: api
                                .canonical_address(&HumanAddr::from(MOCK_TOKEN_CONTRACT_ADDR))
                                .unwrap(),
                        };
                        Ok(to_binary(&to_binary(&pool).unwrap()))
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
