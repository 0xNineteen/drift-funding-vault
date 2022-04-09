use anchor_lang::prelude::*;

// local crates 
pub mod error;
pub mod state;
pub mod instructions;

pub use error::*;
pub use instructions::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod drift_vault {
    use super::*;

    pub fn initialize_vault(
        ctx: Context<InitializeVault>, 
        user_nonce: u8, 
        authority_nonce: u8,
        user_positions_nonce: u8,
    ) -> ProgramResult {
        instructions::initialize_vault(ctx, user_nonce, authority_nonce, user_positions_nonce)
    }

    // ** deposit
    // transfer user USDC => drift collateral   
    // mint pool tokens to user
    pub fn deposit(
        ctx: Context<Deposit>, 
        deposit_amount: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::deposit(ctx, deposit_amount, authority_nonce)
    }

    // ** widthdraw 
    // compute relative collateral to burn_pool_tokens
    // adjust position size:
    //  compute new_collateral = collateral - withdraw_amount 
    //  reduce position so approx 1:1 collateral:liabilities after withdraw
    // transfer from drift vault => vault ATA
    // vault ATA => user ATA  
    // burn user pool_tokens 
    pub fn withdraw(
        ctx: Context<Withdraw>, 
        burn_amount: u128,
        market_index: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::withdraw(ctx, burn_amount, market_index, authority_nonce)
    }

    // ** update position 
    // compute mark price (amm.base_amount ... )
    // compute oracle price 
    // compute funding_rate = mark - oracle 
    // if funding = good for longs => *open_long()
    // if funding = good for shorts => *open_short()
    pub fn update_position(
        ctx: Context<UpdatePosition>, 
        market_index: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::update_position(ctx, market_index, authority_nonce)
    }

}
