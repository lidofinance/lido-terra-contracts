mod tax_querier;

pub use tax_querier::deduct_tax;
pub mod airdrop;
pub mod contract_error;
pub mod hub;
pub mod reward;

#[cfg(test)]
mod mock_querier;

#[cfg(test)]
mod testing;
