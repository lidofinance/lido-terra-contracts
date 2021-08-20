use anchor_basset_validators_registry::registry::Validator as RegistryValidator;
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, to_vec, AllBalanceResponse, Api, BalanceResponse,
    BankQuery, CanonicalAddr, Coin, ContractResult, CustomQuery, Decimal, Empty, FullDelegation,
    OwnedDeps, Querier, QuerierResult, QueryRequest, StdError, StdResult, SystemError,
    SystemResult, Uint128, Validator, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use cw20_base::state::{MinterData, TokenInfo};
use std::collections::HashMap;

use basset::hub::Config;
use serde::de::DeserializeOwned;
use terra_cosmwasm::{TaxCapResponse, TaxRateResponse, TerraQuery, TerraQueryWrapper, TerraRoute};

pub const MOCK_CONTRACT_ADDR: &str = "cosmos2contract";

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = MOCK_CONTRACT_ADDR;
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
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
    token_querier: TokenQuerier,
    balance_querier: BalanceQuerier,
    tax_querier: TaxQuerier,
    validators: Vec<RegistryValidator>,
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
                            QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        TerraQuery::TaxCap { denom } => {
                            let cap = self
                                .tax_querier
                                .caps
                                .get(denom)
                                .copied()
                                .unwrap_or_default();
                            let res = TaxCapResponse { cap };
                            QuerierResult::Ok(ContractResult::from(to_binary(&res)))
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: _,
                msg: _,
            }) => {
                let mut validators = self.validators.clone();
                validators.sort_by(|v1, v2| v1.total_delegated.cmp(&v2.total_delegated));
                QuerierResult::Ok(ContractResult::from(to_binary(&validators)))
            }
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                let prefix_config = to_length_prefixed(b"config").to_vec();
                let prefix_token_inf = to_length_prefixed(b"token_info").to_vec();
                let prefix_balance = to_length_prefixed(b"balance").to_vec();
                let api: MockApi = MockApi::default();

                if key.as_slice().to_vec() == prefix_config {
                    let config = Config {
                        creator: api.addr_canonicalize(&String::from("owner1")).unwrap(),
                        reward_dispatcher_contract: Some(
                            api.addr_canonicalize(&String::from("reward")).unwrap(),
                        ),
                        bluna_token_contract: Some(
                            api.addr_canonicalize(&String::from("token")).unwrap(),
                        ),
                        validators_registry_contract: Some(
                            api.addr_canonicalize(&String::from("validators")).unwrap(),
                        ),
                        stluna_token_contract: Some(
                            api.addr_canonicalize(&String::from("stluna_token"))
                                .unwrap(),
                        ),
                        airdrop_registry_contract: Some(
                            api.addr_canonicalize(&String::from("airdrop")).unwrap(),
                        ),
                    };
                    QuerierResult::Ok(ContractResult::from(to_binary(
                        &to_binary(&config).unwrap(),
                    )))
                } else if key.as_slice().to_vec() == prefix_token_inf {
                    let balances: &HashMap<String, Uint128> =
                        match self.token_querier.balances.get(contract_addr) {
                            Some(balances) => balances,
                            None => {
                                return QuerierResult::Err(SystemError::InvalidRequest {
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
                    let api: MockApi = MockApi::default();
                    let token_inf: TokenInfo = TokenInfo {
                        name: "bluna".to_string(),
                        symbol: "BLUNA".to_string(),
                        decimals: 6,
                        total_supply,
                        mint: Some(MinterData {
                            minter: api.addr_validate(&String::from("hub")).unwrap(),
                            cap: None,
                        }),
                    };
                    QuerierResult::Ok(ContractResult::from(to_binary(&token_inf)))
                } else if key.as_slice()[..prefix_balance.len()].to_vec() == prefix_balance {
                    let key_address: &[u8] = &key.as_slice()[prefix_balance.len()..];
                    let address_raw: CanonicalAddr = CanonicalAddr::from(key_address);
                    let balances: &HashMap<String, Uint128> =
                        match self.token_querier.balances.get(contract_addr) {
                            Some(balances) => balances,
                            None => {
                                return QuerierResult::Err(SystemError::InvalidRequest {
                                    error: format!(
                                        "No balance info exists for the contract {}",
                                        contract_addr
                                    ),
                                    request: key.as_slice().into(),
                                })
                            }
                        };
                    let api: MockApi = MockApi::default();
                    let address: String = match api.addr_humanize(&address_raw) {
                        Ok(v) => v.to_string(),
                        Err(e) => {
                            return QuerierResult::Err(SystemError::InvalidRequest {
                                error: format!("Parsing query request: {}", e),
                                request: key.as_slice().into(),
                            })
                        }
                    };
                    let balance = match balances.get(&address) {
                        Some(v) => v,
                        None => {
                            return QuerierResult::Err(SystemError::InvalidRequest {
                                error: "Balance not found".to_string(),
                                request: key.as_slice().into(),
                            })
                        }
                    };
                    QuerierResult::Ok(ContractResult::from(to_binary(&balance)))
                } else {
                    unimplemented!()
                }
            }
            QueryRequest::Bank(BankQuery::AllBalances { address }) => {
                if address == &String::from("reward") {
                    let mut coins: Vec<Coin> = vec![];
                    let luna = Coin {
                        denom: "uluna".to_string(),
                        amount: Uint128::from(1000u128),
                    };
                    coins.push(luna);
                    let krt = Coin {
                        denom: "ukrt".to_string(),
                        amount: Uint128::from(1000u128),
                    };
                    coins.push(krt);
                    let usd = Coin {
                        denom: "uusd".to_string(),
                        amount: Uint128::from(1000u128),
                    };
                    coins.push(usd);
                    let all_balances = AllBalanceResponse { amount: coins };
                    QuerierResult::Ok(ContractResult::from(to_binary(&all_balances)))
                } else {
                    unimplemented!()
                }
            }
            QueryRequest::Bank(BankQuery::Balance { address, denom }) => {
                if address == &String::from(MOCK_CONTRACT_ADDR) && denom == "uluna" {
                    match self
                        .balance_querier
                        .balances
                        .get(&String::from(MOCK_CONTRACT_ADDR))
                    {
                        Some(coin) => {
                            QuerierResult::Ok(ContractResult::from(to_binary(&BalanceResponse {
                                amount: Coin {
                                    denom: coin.denom.clone(),
                                    amount: coin.amount,
                                },
                            })))
                        }
                        None => QuerierResult::Err(SystemError::InvalidRequest {
                            error: "balance not found".to_string(),
                            request: Default::default(),
                        }),
                    }
                } else if address == &String::from("reward") && denom == "uusd" {
                    let bank_res = BalanceResponse {
                        amount: Coin {
                            amount: Uint128::from(2000u128),
                            denom: denom.to_string(),
                        },
                    };
                    QuerierResult::Ok(ContractResult::from(to_binary(&bank_res)))
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

    pub fn query<T: DeserializeOwned>(&self, request: &QueryRequest<Empty>) -> StdResult<T> {
        self.custom_query(request)
    }

    /// Makes the query and parses the response. Also handles custom queries,
    /// so you need to specify the custom query type in the function parameters.
    /// If you are no using a custom query, just use `query` for easier interface.
    ///
    /// Any error (System Error, Error or called contract, or Parse Error) are flattened into
    /// one level. Only use this if you don't need to check the SystemError
    /// eg. If you don't differentiate between contract missing and contract returned error
    pub fn custom_query<C: CustomQuery, U: DeserializeOwned>(
        &self,
        request: &QueryRequest<C>,
    ) -> StdResult<U> {
        let raw = to_vec(request).map_err(|serialize_err| {
            StdError::generic_err(format!("Serializing QueryRequest: {}", serialize_err))
        })?;
        match self.raw_query(&raw) {
            SystemResult::Err(system_err) => Err(StdError::generic_err(format!(
                "Querier system error: {}",
                system_err
            ))),
            SystemResult::Ok(ContractResult::Err(contract_err)) => Err(StdError::generic_err(
                format!("Querier contract error: {}", contract_err),
            )),
            SystemResult::Ok(ContractResult::Ok(value)) => from_binary(&value),
        }
    }
}

#[derive(Clone, Default)]
pub struct BalanceQuerier {
    balances: HashMap<String, Coin>,
}

impl BalanceQuerier {
    pub fn new(balances: &[(String, Coin)]) -> Self {
        BalanceQuerier {
            balances: native_balances_to_map(balances),
        }
    }
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    balances: HashMap<String, HashMap<String, Uint128>>,
}

impl TokenQuerier {
    pub fn new(balances: &[(&String, &[(&String, &Uint128)])]) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
        }
    }
}

pub(crate) fn native_balances_to_map(balances: &[(String, Coin)]) -> HashMap<String, Coin> {
    let mut balances_map: HashMap<String, Coin> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let coin = Coin {
            denom: balances.clone().denom,
            amount: balances.clone().amount,
        };
        balances_map.insert(String::from(contract_addr), coin);
    }
    balances_map
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128)])],
) -> HashMap<String, HashMap<String, Uint128>> {
    let mut balances_map: HashMap<String, HashMap<String, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), **balance);
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            tax_querier: TaxQuerier::default(),
            balance_querier: BalanceQuerier::default(),
            validators: vec![],
        }
    }

    pub fn with_native_balances(&mut self, balances: &[(String, Coin)]) {
        self.balance_querier = BalanceQuerier::new(balances);
    }

    // configure the mint whitelist mock basset
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances);
    }

    // configure the tax mock querier
    pub fn _with_tax(&mut self, rate: Decimal, caps: &[(&String, &Uint128)]) {
        self.tax_querier = TaxQuerier::_new(rate, caps);
    }

    pub fn add_validator(&mut self, validator: RegistryValidator) {
        self.validators.push(validator);
    }
}
