use crate::pda::*;
use crate::dlmm::accounts::*;
use crate::extensions::bin_array::BinArrayExtension;
use anchor_lang::prelude::*;
use crate::constants::CustomError;

pub trait PositionExtension {
    fn get_bin_array_indexes_bound(&self) -> Result<(i32, i32)>;
    fn get_bin_array_keys_coverage(&self, keys: &mut Vec<Pubkey>) -> Result<()>;
    fn get_bin_array_accounts_meta_coverage(&self) -> Result<Vec<AccountMeta>>;

    fn get_bin_array_indexes_bound_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<(i32, i32)>;

    fn get_bin_array_keys_coverage_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
        keys: &mut Vec<Pubkey>,
    ) -> Result<()>;

    fn get_bin_array_accounts_meta_coverage_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<Vec<AccountMeta>>;

    fn is_empty(&self) -> bool;
}

impl PositionExtension for PositionV2 {
    fn get_bin_array_indexes_bound(&self) -> Result<(i32, i32)> {
        self.get_bin_array_indexes_bound_by_chunk(self.lower_bin_id, self.upper_bin_id)
    }

    fn get_bin_array_indexes_bound_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<(i32, i32)> {
        require!(lower_bin_id >= self.lower_bin_id && upper_bin_id <= self.upper_bin_id,
            CustomError::RequestedBinIdRangeOutOfBounds
        );
        let lower_bin_array_index = BinArray::bin_id_to_bin_array_index(lower_bin_id)?;
        let upper_bin_array_index = lower_bin_array_index + 1;
        Ok((lower_bin_array_index, upper_bin_array_index))
    }

    // Add ref to keys to save on allocations
    fn get_bin_array_keys_coverage(&self, keys: &mut Vec<Pubkey>) -> Result<()> {
        self.get_bin_array_keys_coverage_by_chunk(self.lower_bin_id, self.upper_bin_id, keys)
    }

    fn get_bin_array_keys_coverage_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
        keys: &mut Vec<Pubkey>,
    ) -> Result<()> {
        let (lower_bin_array_index, upper_bin_array_index) =
            self.get_bin_array_indexes_bound_by_chunk(lower_bin_id, upper_bin_id)?;
        for bin_array_index in lower_bin_array_index..=upper_bin_array_index {
            keys.push(derive_bin_array_pda(self.lb_pair, bin_array_index.into()).0);
        }
        Ok(())
    }

    fn get_bin_array_accounts_meta_coverage(&self) -> Result<Vec<AccountMeta>> {
        self.get_bin_array_accounts_meta_coverage_by_chunk(self.lower_bin_id, self.upper_bin_id)
    }

    fn get_bin_array_accounts_meta_coverage_by_chunk(
        &self,
        lower_bin_id: i32,
        upper_bin_id: i32,
    ) -> Result<Vec<AccountMeta>> {
        let mut bin_array_keys: Vec<Pubkey> = Vec::new();
        self.get_bin_array_keys_coverage_by_chunk(lower_bin_id, upper_bin_id, &mut bin_array_keys)?;
        Ok(bin_array_keys
            .into_iter()
            .map(|key| AccountMeta::new(key, false))
            .collect())
    }

    fn is_empty(&self) -> bool {
        for i in 0..self.liquidity_shares.len() {
            if self.liquidity_shares[i] != 0 {
                return false;
            }

            if self.fee_infos[i].fee_x_pending != 0 || self.fee_infos[i].fee_y_pending != 0 {
                return false;
            }

            for pending_reward in self.reward_infos[i].reward_pendings {
                if pending_reward != 0 {
                    return false;
                }
            }
        }

        true
    }
}
