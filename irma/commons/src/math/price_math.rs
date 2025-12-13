use crate::*;
// use anchor_lang::prelude::msg;

// This function calculates the price from the active_id and bin_step.
// Price = (1 + bin_step / BASIS_POINT_MAX) ^ active_id
pub fn get_price_from_id(active_id: i32, bin_step: u16) -> Result<u128> {

    let bps = u128::from(bin_step)
        .checked_shl(SCALE_OFFSET.into())
        .unwrap()
        .checked_div(BASIS_POINT_MAX as u128)
        .unwrap();

    let base = ONE
        .checked_add(bps)
        .unwrap();

    // msg!("-- get_price_from_id: active_id {}, bin_step {}, bps {}, base {}", active_id, bin_step, bps, base);

    Ok(pow(base, active_id).unwrap())
}
