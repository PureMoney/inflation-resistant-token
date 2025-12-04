use anchor_lang::error_code;

#[error_code]
pub enum CustomError {
    #[msg("Bin array is not correct")]
    BinArrayIsNotCorrect,
    #[msg("Amount x is not zero")]
    AmountXNotZero,
    #[msg("Amount y is not zero")]
    AmountYNotZero,
    #[msg("Cannot get binarray")]
    CannotGetBinArray,
    #[msg("Bin is not within the position")]
    BinIsNotWithinThePosition,
    // #[msg("Account data too small")]
    // AccountDataTooSmall,
    #[msg("Missing lb pair state")]
    MissingLbPairState,
    #[msg("Position not found")]
    PositionNotFound,
    #[msg("Bin array not found.")]
    BinArrayNotFound,
    #[msg("Invalid amount provided.")]
    InvalidAmount,
    // #[msg("Mint price not set.")]
    // MintPriceNotSet,
    #[msg("Invalid quote token.")]
    InvalidQuoteToken,
    #[msg("Insufficient circulation.")]
    InsufficientCirculation,
    #[msg("Insufficient reserve.")]
    InsufficientReserve,
    #[msg("Invalid reserve value.")]
    InvalidBacking,
    #[msg("Invalid IRMA amount.")]
    InvalidIrmaAmount,
    #[msg("No reserve list.")]
    InvalidReserveList,
    #[msg("Invalid backing symbol.")]
    InvalidBackingSymbol,
    #[msg("Invalid backing address.")]
    InvalidBackingAddress,
    #[msg("Symbol not found.")]
    SymbolNotFound,
    #[msg("Lb pair state not found")]
    LbPairStateNotFound,
    #[msg("Ordering of stablecoins does not match LbPair order in remaining_accounts.")]
    LbPairOrderingMismatch,
    #[msg("Goodbye USD: Reserve stablecoin price too high.")]
    GoodbyeUSD,
    #[msg("This reserve stablecoin has lost too much value, remove it.")]
    RemoveReserve,
    #[msg("Failed to fetch bin arrays.")]
    FailedToFetchBinArrays,
}
