use crate::deduct_tax;
use crate::mock_querier::mock_dependencies;
use cosmwasm_std::{Coin, Decimal, Uint128};

#[test]
fn test_deduct_tax() {
    let mut deps = mock_dependencies(20, &[]);

    deps.querier.with_tax(
        Decimal::percent(1),
        &[(&"uusd".to_string(), &Uint128::from(1000000u128))],
    );

    // cap to 1000000
    assert_eq!(
        deduct_tax(&deps, Coin::new(10000000000u128, "uusd")).unwrap(),
        Coin {
            denom: "uusd".to_string(),
            amount: Uint128(9999000000u128)
        }
    );

    // normal tax
    assert_eq!(
        deduct_tax(&deps, Coin::new(50000000u128, "uusd")).unwrap(),
        Coin {
            denom: "uusd".to_string(),
            amount: Uint128(49504950u128)
        }
    );
}
