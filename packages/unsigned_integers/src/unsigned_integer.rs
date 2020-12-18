use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(
    Serialize, Deserialize, Copy, Clone, Default, Debug, PartialEq, Eq, PartialOrd, Ord, JsonSchema,
)]
pub struct UnsignedInt(#[schemars(with = "String")] pub Uint128);

impl UnsignedInt {
    pub fn from_subtraction<A: Into<Uint128>, B: Into<Uint128>>(
        minuend: A,
        subtrahend: B,
    ) -> UnsignedInt {
        let minuend: Uint128 = minuend.into();
        let subtrahend: Uint128 = subtrahend.into();
        let subtraction = minuend - subtrahend;
        if subtraction.is_err() {
            return UnsignedInt((subtrahend - minuend).unwrap());
        }
        UnsignedInt(subtraction.unwrap())
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
        let unsigned_integer = UnsignedInt::from_subtraction(min, sub);
        assert_eq!(unsigned_integer.0, Uint128(10));

        //check negative values
        let min = Uint128(1000000);
        let sub = Uint128(1100000);
        let unsigned_integer = UnsignedInt::from_subtraction(min, sub);
        assert_eq!(unsigned_integer.0, Uint128(100000));
    }
}
