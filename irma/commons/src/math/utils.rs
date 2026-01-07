use anchor_lang::prelude::*;
use crate::u128x128_math::{mul_div, mul_shr, shl_div};
use crate::dlmm::types::Rounding;
use num_traits::FromPrimitive;
use crate::CustomError;

#[inline]
pub fn safe_mul_shr_cast<T: FromPrimitive>(
    x: u128,
    y: u128,
    offset: u8,
    rounding: Rounding,
) -> Result<T> {
    Ok(T::from_u128(mul_shr(x, y, offset, rounding).expect("Option::None")).expect("overflow"))
}

#[inline]
pub fn safe_shl_div_cast<T: FromPrimitive>(
    x: u128,
    y: u128,
    offset: u8,
    rounding: Rounding,
) -> Result<T> {
    Ok(T::from_u128(shl_div(x, y, offset, rounding).ok_or(CustomError::MathError)?).ok_or(CustomError::MathError)?)
}

pub fn safe_mul_div_cast<T: FromPrimitive>(
    x: u128,
    y: u128,
    denominator: u128,
    rounding: Rounding,
) -> Result<T> {
    Ok(T::from_u128(mul_div(x, y, denominator, rounding).expect("Option::None")).expect("overflow"))
}
