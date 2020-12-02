use std::env::current_dir;
use std::fs::create_dir_all;

use anchor_basset_reward::hook::InitHook;
use anchor_basset_reward::init::RewardInitMsg;
use anchor_basset_reward::msg::{HandleMsg, QueryMsg, TokenInfoResponse};
use anchor_basset_reward::state::{Config, Index, Parameters};
use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InitHook), &out_dir);
    export_schema(&schema_for!(RewardInitMsg), &out_dir);
    export_schema(&schema_for!(HandleMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(TokenInfoResponse), &out_dir);
    export_schema(&schema_for!(Parameters), &out_dir);
    export_schema(&schema_for!(Index), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
}
