use anchor_lang::prelude::*;

#[error]
pub enum VaultErrorCode {
    #[msg("Not enough funds.")]
    NotEnoughFunds,
    #[msg("Widthdraw amount too small.")]
    WidthdrawAmountTooSmall,
}

// copy pasta from clearing house 
#[macro_export]
macro_rules! math_error {
    () => {{
        || {
            let error_code = ErrorCode::MathError;
            msg!("Error {} thrown at {}:{}", error_code, file!(), line!());
            error_code
        }
    }};
}
