use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use anchor_basset_hub::msg::{
    ExchangeRateResponse, InitMsg, QueryMsg, TotalBondedResponse, UnbondEpochsResponse,
    UnbondRequestsResponse, WhitelistedValidatorsResponse, WithdrawableUnbondedResponse,
};
use anchor_basset_hub::state::{Config, EpochId, MsgStatus, Parameters};
use hub_courier::{HandleMsg, PoolInfo};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InitMsg), &out_dir);
    export_schema(&schema_for!(HandleMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(PoolInfo), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(Parameters), &out_dir);
    export_schema(&schema_for!(MsgStatus), &out_dir);
    export_schema(&schema_for!(EpochId), &out_dir);
    export_schema(&schema_for!(ExchangeRateResponse), &out_dir);
    export_schema(&schema_for!(WhitelistedValidatorsResponse), &out_dir);
    export_schema(&schema_for!(WithdrawableUnbondedResponse), &out_dir);
    export_schema(&schema_for!(TotalBondedResponse), &out_dir);
    export_schema(&schema_for!(UnbondRequestsResponse), &out_dir);
    export_schema(&schema_for!(UnbondEpochsResponse), &out_dir);
}
