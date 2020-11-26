mod tax_querier;

pub use tax_querier::deduct_tax;

#[cfg(test)]
mod mock_querier;

#[cfg(test)]
mod testing;
