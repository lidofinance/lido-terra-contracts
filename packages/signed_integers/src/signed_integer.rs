use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// boolean is set for specifying the negativity.
/// false means the value is positive.
#[derive(
    Serialize, Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, JsonSchema,
)]
pub struct SignedInt(#[schemars(with = "String")] pub Uint128, pub bool);

impl SignedInt {
    pub fn from_subtraction<A: Into<Uint128>, B: Into<Uint128>>(
        minuend: A,
        subtrahend: B,
    ) -> SignedInt {
        let minuend: Uint128 = minuend.into();
        let subtrahend: Uint128 = subtrahend.into();
        let subtraction = minuend - subtrahend;
        if subtraction.is_err() {
            return SignedInt((subtrahend - minuend).unwrap(), true);
        }
        SignedInt(subtraction.unwrap(), false)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use cosmwasm_std::Uint128;

    #[test]
    fn from_subtraction() {
        let min = Uint128(1000010);
        let sub = Uint128(1000000);
        let signed_integer = SignedInt::from_subtraction(min, sub);
        assert_eq!(signed_integer.0, Uint128(10));
        assert_eq!(signed_integer.1, false);

        //check negative values
        let min = Uint128(1000000);
        let sub = Uint128(1100000);
        let signed_integer = SignedInt::from_subtraction(min, sub);
        assert_eq!(signed_integer.0, Uint128(100000));
        assert_eq!(signed_integer.1, true);
    }
}
