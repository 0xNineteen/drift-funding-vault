use anchor_lang::prelude::*;

// local crates 
pub mod error;
pub mod state;
pub mod instructions;

pub use error::*;
pub use instructions::*;

declare_id!("FKKbXdAxoX6RK6h2ESspJEgxfN83JHw48CYfh1if142Z");

#[program]
pub mod drift_vault {
    use super::*;

    // ** initialize 
    // 1. create pool mint for LPs 
    // 2. create vault collateral ATA 
    // 3. create drift account 
    pub fn initialize_vault(
        ctx: Context<InitializeVault>, 
        user_nonce: u8, 
        authority_nonce: u8,
        user_positions_nonce: u8,
    ) -> ProgramResult {
        instructions::initialize_vault(ctx, user_nonce, authority_nonce, user_positions_nonce)
    }

    // ** deposit
    // 1. mint pool tokens to user
    // 2. deposit usdc to vault's drift collateral 
    pub fn deposit(
        ctx: Context<Deposit>, 
        deposit_amount: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::deposit(ctx, deposit_amount, authority_nonce)
    }

    // ** widthdraw 
    // 1. compute relative collateral to burn_pool_tokens
    // 2. adjust position size:
    //  compute new_collateral = collateral - withdraw_amount 
    //  reduce position so approx 1:1 collateral:liabilities after withdraw
    // 3. transfer from drift vault => vault ATA
    // 4. vault ATA => user ATA  
    // 5. burn user pool_tokens 
    pub fn withdraw(
        ctx: Context<Withdraw>, 
        burn_amount: u128,
        market_index: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::withdraw(ctx, burn_amount, market_index, authority_nonce)
    }

    // ** update position 
    // 1. compute funding_rate = mark - oracle 
    // 2. do:
    //  if funding = good for longs => *open_long()
    //  if funding = good for shorts => *open_short()
    // we aim for 1:1 ratio of collateral + positions
    pub fn update_position(
        ctx: Context<UpdatePosition>, 
        market_index: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        instructions::update_position(ctx, market_index, authority_nonce)
    }

}
