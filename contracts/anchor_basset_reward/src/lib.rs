pub mod contracts;
pub mod hook;
pub mod init;
pub mod msg;
pub mod state;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points!(contracts);
