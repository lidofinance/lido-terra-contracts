use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{
    from_slice, to_binary, Api, Coin, Empty, Extern, FullDelegation, HumanAddr, Querier,
    QuerierResult, QueryRequest, SystemError, Validator, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use gov_courier::PoolInfo;

pub const MOCK_CONTRACT_ADDR: &str = "governance";

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
    base: MockQuerier<Empty>,
    canonical_length: usize,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
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
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Raw {
                contract_addr: _,
                key,
            }) => {
                let prefix_pool = to_length_prefixed(b"pool_info").to_vec();
                let api: MockApi = MockApi::new(self.canonical_length);
                if key.as_slice().to_vec() == prefix_pool {
                    let pool = PoolInfo {
                        exchange_rate: Default::default(),
                        total_bond_amount: Default::default(),
                        last_index_modification: 0,
                        reward_account: api.canonical_address(&HumanAddr::from("reward")).unwrap(),
                        is_reward_exist: true,
                        is_token_exist: true,
                        token_account: api.canonical_address(&HumanAddr::from("token")).unwrap(),
                    };
                    Ok(to_binary(&to_binary(&pool).unwrap()))
                } else {
                    unimplemented!()
                }
            }
            _ => self.base.handle_query(request),
        }
    }
    pub fn update_staking(
        &mut self,
        denom: &str,
        validators: &[Validator],
        delegations: &[FullDelegation],
    ) {
        self.base.update_staking(denom, validators, delegations);
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>, canonical_length: usize) -> Self {
        WasmMockQuerier {
            base,
            canonical_length,
        }
    }
}
