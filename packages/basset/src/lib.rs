mod tax_querier;

pub use tax_querier::{compute_lido_fee, deduct_tax};

#[cfg(test)]
mod mock_querier;

#[cfg(test)]
mod testing;
