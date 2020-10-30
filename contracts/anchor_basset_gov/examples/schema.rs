use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use anchor_bluna::msg::InitMsg;
use anchor_bluna::state::GovConfig;
use gov_courier::{HandleMsg, PoolInfo};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InitMsg), &out_dir);
    export_schema(&schema_for!(HandleMsg), &out_dir);
    export_schema(&schema_for!(PoolInfo), &out_dir);
    export_schema(&schema_for!(GovConfig), &out_dir);
}
