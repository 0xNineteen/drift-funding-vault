use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod drift_vault {
    use super::*;
    pub fn initialize_vault(ctx: Context<InitializeVault>) -> ProgramResult {

        // create new pool token_mint (pool_mint)
        
        // create new open-orders drift account with PDA (?) -- probably want 
        // to do once with the JS SDK for easy setup 
        // create vault (TokenAcocunt = USDC) (?) -- probs above too
        // maybe can call INIT on account with 

        Ok(())
    }

    // ** deposit
    // transfer user USDC => vault collateral TokenAccount 
    // mint pool tokens 

    // ** widthdraw 
    // burn user pool_tokens 
    // close position size (relative to amount of burn_pool_tokens)
    // transfer from vault => user 
    // emit widthdraw event 

}

#[derive(Accounts)]
pub struct InitializeVault {}
