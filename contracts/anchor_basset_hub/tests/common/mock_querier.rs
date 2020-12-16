use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{
    from_slice, to_binary, AllBalanceResponse, Api, BalanceResponse, BankQuery, CanonicalAddr,
    Coin, Decimal, Extern, FullDelegation, HumanAddr, Querier, QuerierResult, QueryRequest,
    SystemError, Uint128, Validator, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20_base::state::{MinterData, TokenInfo};
use gov_courier::PoolInfo;
use std::collections::HashMap;

use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub fn mock_dependencies(
    canonical_length: usize,
    contract_balance: &[Coin],
) -> Extern<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(
        MockQuerier::new(&[(&contract_addr, contract_balance)]),
        canonical_length,
        MockApi::new(canonical_length),
    );

    Extern {
        storage: MockStorage::default(),
        api: MockApi::new(canonical_length),
        querier: custom_querier,
    }
}

#[derive(Clone, Default)]
pub struct TaxQuerier {
    rate: Decimal,
    caps: HashMap<String, Uint128>,
}

impl TaxQuerier {
    pub fn _new(rate: Decimal, caps: &[(&String, &Uint128)]) -> Self {
        TaxQuerier {
            rate,
            caps: _caps_to_map(caps),
        }
    }
}

pub(crate) fn _caps_to_map(caps: &[(&String, &Uint128)]) -> HashMap<String, Uint128> {
    let mut owner_map: HashMap<String, Uint128> = HashMap::new();
    for (denom, cap) in caps.iter() {
        owner_map.insert(denom.to_string(), **cap);
    }
    owner_map
}

pub struct WasmMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    canonical_length: usize,
    token_querier: TokenQuerier,
    tax_querier: TaxQuerier,
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
                                rate: self.tax_querier.rate,
                            };
                            Ok(to_binary(&res))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
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
                let prefix_pool = to_length_prefixed(b"pool_info").to_vec();
                let prefix_token_inf = to_length_prefixed(b"token_info").to_vec();
                let prefix_balance = to_length_prefixed(b"balance").to_vec();
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
                } else if key.as_slice().to_vec() == prefix_token_inf {
                    let balances: &HashMap<HumanAddr, Uint128> =
                        match self.token_querier.balances.get(contract_addr) {
                            Some(balances) => balances,
                            None => {
                                return Err(SystemError::InvalidRequest {
                                    error: format!(
                                        "No balance info exists for the contract {}",
                                        contract_addr
                                    ),
                                    request: key.as_slice().into(),
                                })
                            }
                        };
                    let mut total_supply = Uint128::zero();

                    for balance in balances {
                        total_supply += *balance.1;
                    }
                    let api: MockApi = MockApi::new(self.canonical_length);
                    let token_inf: TokenInfo = TokenInfo {
                        name: "bluna".to_string(),
                        symbol: "BLUNA".to_string(),
                        decimals: 6,
                        total_supply,
                        mint: Some(MinterData {
                            minter: api
                                .canonical_address(&HumanAddr::from("governance"))
                                .unwrap(),
                            cap: None,
                        }),
                    };
                    Ok(to_binary(&to_binary(&token_inf).unwrap()))
                } else if key.as_slice()[..prefix_balance.len()].to_vec() == prefix_balance {
                    let key_address: &[u8] = &key.as_slice()[prefix_balance.len()..];
                    let address_raw: CanonicalAddr = CanonicalAddr::from(key_address);
                    let balances: &HashMap<HumanAddr, Uint128> =
                        match self.token_querier.balances.get(contract_addr) {
                            Some(balances) => balances,
                            None => {
                                return Err(SystemError::InvalidRequest {
                                    error: format!(
                                        "No balance info exists for the contract {}",
                                        contract_addr
                                    ),
                                    request: key.as_slice().into(),
                                })
                            }
                        };
                    let api: MockApi = MockApi::new(self.canonical_length);
                    let address: HumanAddr = match api.human_address(&address_raw) {
                        Ok(v) => v,
                        Err(e) => {
                            return Err(SystemError::InvalidRequest {
                                error: format!("Parsing query request: {}", e),
                                request: key.as_slice().into(),
                            })
                        }
                    };
                    let balance = match balances.get(&address) {
                        Some(v) => v,
                        None => {
                            return Err(SystemError::InvalidRequest {
                                error: "Balance not found".to_string(),
                                request: key.as_slice().into(),
                            })
                        }
                    };
                    Ok(to_binary(&to_binary(&balance).unwrap()))
                } else {
                    unimplemented!()
                }
            }
            QueryRequest::Bank(BankQuery::AllBalances { address }) => {
                if address == &HumanAddr::from("reward") {
                    let mut coins: Vec<Coin> = vec![];
                    let luna = Coin {
                        denom: "uluna".to_string(),
                        amount: Uint128(1000u128),
                    };
                    coins.push(luna);
                    let krt = Coin {
                        denom: "ukrt".to_string(),
                        amount: Uint128(1000u128),
                    };
                    coins.push(krt);
                    let usd = Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128(1000u128),
                    };
                    coins.push(usd);
                    let all_balances = AllBalanceResponse { amount: coins };
                    Ok(to_binary(&all_balances))
                } else {
                    unimplemented!()
                }
            }
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                if address == &HumanAddr::from("reward") && denom == "uusd" {
                    let bank_res = BalanceResponse {
                        amount: Coin {
                            amount: Uint128(2000u128),
                            denom: denom.to_string(),
                        },
                    };
                    Ok(to_binary(&bank_res))
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

#[derive(Clone, Default)]
pub struct TokenQuerier {
    balances: HashMap<HumanAddr, HashMap<HumanAddr, Uint128>>,
}

impl TokenQuerier {
    pub fn new(balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])]) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])],
) -> HashMap<HumanAddr, HashMap<HumanAddr, Uint128>> {
    let mut balances_map: HashMap<HumanAddr, HashMap<HumanAddr, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<HumanAddr, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(HumanAddr::from(addr), **balance);
        }

        balances_map.insert(HumanAddr::from(contract_addr), contract_balances_map);
    }
    balances_map
}

impl WasmMockQuerier {
    pub fn new<A: Api>(
        base: MockQuerier<TerraQueryWrapper>,
        canonical_length: usize,
        _api: A,
    ) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            canonical_length,
            tax_querier: TaxQuerier::default(),
        }
    }

    // configure the mint whitelist mock basset
    pub fn with_token_balances(&mut self, balances: &[(&HumanAddr, &[(&HumanAddr, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances);
    }

    // configure the tax mock querier
    pub fn _with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::_new(rate, caps);
    }
}
