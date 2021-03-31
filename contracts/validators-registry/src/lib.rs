pub mod common;
pub mod contract;
pub mod msg;
pub mod registry;

#[cfg(all(target_arch = "wasm32", not(feature = "library")))]
cosmwasm_std::create_entry_points!(contract);
