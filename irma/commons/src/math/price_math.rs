use crate::*;
use anchor_lang::prelude::msg;

// TODO: make sure this is correct. 
// The old routine was getting the power of (1 + bin_step / BASIS_POINT_MAX) raised to active_id
// and getting overflow most of the time.
// Here we are just multiplying (1 + bin_step / BASIS_POINT_MAX) with active_id.
// What happens when active_id is negative?
pub fn get_price_from_id(active_id: i32, bin_step: u16) -> Result<u128> {

    msg!("1. get_price_from_id: active_id {}, bin_step {}", active_id, bin_step);

    let bps = u128::from(bin_step)
        .checked_shl(SCALE_OFFSET.into())
        .unwrap()
        // .context("overflow")?
        .checked_div(BASIS_POINT_MAX as u128)
        .unwrap();
        // .context("overflow")?;
        // resulting bps = 2621 for bin_step = 25

    let base = ONE
        .checked_add(bps)
        .unwrap()
        .checked_mul(active_id as u128)
        .unwrap(); // .context("overflow")?;

    msg!("2. get_price_from_id: active_id {}, bin_step {}, bps {}, base {}", active_id, bin_step, bps, base);

    Ok(base)

    // Ok(pow(base, active_id).unwrap()) // .context("overflow")
}
