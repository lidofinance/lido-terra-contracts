pub mod hook;
pub mod init;

#[cfg(target_arch = "wasm32")]
cosmwasm_std::create_entry_points!(contract);
