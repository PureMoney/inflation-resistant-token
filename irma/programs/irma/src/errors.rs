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
    #[msg("Account not found")]
    AccountNotFound,
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
    #[msg("Missing required account for instruction")]
    MissingRequiredAccount,
    #[msg("Ordering of stablecoins does not match LbPair order in remaining_accounts.")]
    LbPairOrderingMismatch,
    #[msg("Goodbye USD: Reserve stablecoin price too high.")]
    GoodbyeUSD,
    #[msg("This reserve stablecoin has lost too much value, remove it.")]
    RemoveReserve,
    #[msg("Failed to fetch bin arrays.")]
    FailedToFetchBinArrays,
    #[msg("Configuration must be empty.")]
    ConfigMustBeEmpty,
    #[msg("Reserve not found.")]
    ReserveNotFound,
    #[msg("Invalid pubkey.")]
    InvalidPubkey,
    #[msg("Invalid LbPair state.")]
    InvalidLbPairState,
    #[msg("For IRMA, two-sided liquidity provision is not supported.")]
    InvalidDepositAmounts,
    #[msg("Reserve list and position list mismatch.")]
    ReserveListPositionListMismatch,
    #[msg("IRMA positions must be single-bin.")]
    PositionNotSingleBin,
    #[msg("Too many positions for pair.")]
    TooManyPositionsForPair,
    #[msg("Math error occurred.")]
    MathError,
    #[msg("Invalid MarketMakingMode for IRMA")]
    InvalidMarketMakingModeForIRMA,
    #[msg("Price not found in LB pair")]
    PriceNotFoundInLBPair,
    #[msg("Position missing in remaining_accounts")]
    MissingPositionState,
    #[msg("Found BinArray in remaining_accounts, but it is invalid")]
    InvalidBinArrayState,
    #[msg("Account deserialization failed")]
    AccountDeserializationFailed,
    #[msg("Account borrowing failed")]
    AccountBorrowFailed,
    #[msg("Invalid account data length")]
    InvalidAccountDataLength, // InvalidAccountData
    #[msg("Missing bitmap extension")]
    MissingBitmapExtension,
    #[msg("Invalid account data")]
    InvalidAccountData,
    #[msg("Reserve coin missing decimals")]
    ReserveCoinMissingDecimals,
    #[msg("Invalid number of positions")]
    InvalidNumberOfPositions,
    #[msg("Duplicate positions found for either minting or redeeming")]
    DuplicatePositions,
    #[msg("Too many positions for pair.")]
    TooManyPositions,
    #[msg("Inconsistent positions found.")]
    InconsistentPositionsFound,
    #[msg("Single position not found for the given LB pair.")]
    SinglePositionNotFound,
    #[msg("Price conversion error.")]
    PriceConversionError,
    #[msg("BinArray account not found in remaining accounts.")]
    MissingBinArrayState,
    #[msg("Additional position required.")]
    AdditionalPositionRequired,
    #[msg("Mint position not found.")]
    MintPositionNotFound,
    #[msg("Redeem position not found.")]
    RedeemPositionNotFound,
    #[msg("Position does not belong to the specified LbPair.")]
    PositionPairMismatch,
    #[msg("Position does not belong to the specified owner.")]
    UnauthorizedPositionAccess,
    #[msg("Bin is out of range for the position.")]
    BinOutOfRange,
}
