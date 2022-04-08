use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use clearing_house::context::{
    InitializeUserOptionalAccounts,
    ManagePositionOptionalAccounts as ClearingHouseManagePositionOptionalAccounts,
};
use clearing_house::controller::position::PositionDirection as ClearingHousePositionDirection;
use clearing_house::cpi::accounts::{
    ClosePosition as ClearingHouseClosePosition,
    DepositCollateral as ClearingHouseDepositCollateral, InitializeUserWithExplicitPayer,
    OpenPosition as ClearingHouseOpenPosition,
    WithdrawCollateral as ClearingHouseWithdrawCollateral,
};
use clearing_house::state::state::State;
use clearing_house::program::ClearingHouse;
use clearing_house::state::history::funding_rate::FundingRateHistory;
use clearing_house::state::history::trade::TradeHistory;
use clearing_house::state::{
    history::{deposit::DepositHistory, funding_payment::FundingPaymentHistory},
    market::Markets,
    user::{User, UserPositions},
};


declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod drift_vault {
    use super::*;
    pub fn initialize_vault(
        ctx: Context<InitializeUser>, 
        user_nonce: u8, 
        authority_nonce: u8,
        user_positions_nonce: u8,
    ) -> ProgramResult {

        // create pool mint for LPs [done by anchor]

        // create drift account for vault 
        let authority_seeds = [
            b"authority".as_ref(),
            &[authority_nonce][..],
        ];
        let user_positions_seeds = [
            b"user_positions".as_ref(),
            &[user_positions_nonce][..],
        ];
        let signers = &[&authority_seeds[..], &user_positions_seeds[..]];

        let cpi_program = ctx.accounts.clearing_house_program.to_account_info();
        let cpi_accounts = InitializeUserWithExplicitPayer {
            state: ctx.accounts.clearing_house_state.to_account_info(), // CH
            user: ctx.accounts.clearing_house_user.to_account_info(), // PDA
            user_positions: ctx.accounts.clearing_house_user_positions.clone(), // KP 
            authority: ctx.accounts.authority.clone(), // KP 
            payer: ctx.accounts.payer.clone(), // KP 
            rent: ctx.accounts.rent.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
        };
        
        let cpi_ctx = CpiContext::new_with_signer(
            cpi_program, 
            cpi_accounts,
            signers,
        );

        clearing_house::cpi::initialize_user_with_explicit_payer(
            cpi_ctx,
            user_nonce,
            InitializeUserOptionalAccounts {
                whitelist_token: false,
            },
        )?;

        Ok(())
    }

    // ** deposit
    // transfer user USDC => drift collateral   
    // mint pool tokens 
    // * update position ()

    // ** widthdraw 
    // burn user pool_tokens 
    // close position size (relative to amount of burn_pool_tokens)
    // transfer from vault => user 
    // emit widthdraw event 
    // * update position ()

    // ** update position 
    // compute mark price (amm.base_amount ... )
    // compute oracle price 
    // compute funding_rate = mark - oracle 
    // if funding = good for longs => *open_long()
    // if funding = good for shorts => *open_short()

    // ** open_long/short 
    // compute current position 
    // compute current amount of collateral:position
    // var = amount of leverage 
    // enough to open a new long ? { cpi:long/short }

}



#[derive(Accounts)]
pub struct InitializeUser<'info> {
    #[account(mut, signer)]
    pub payer: AccountInfo<'info>,

    // drift account 
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    pub clearing_house_state: AccountInfo<'info>,
    #[account(mut)]
    pub clearing_house_user: AccountInfo<'info>,
    #[account(mut, seeds = [b"user_positions".as_ref()], bump)]
    pub clearing_house_user_positions: AccountInfo<'info>,

    // pool mint for LPs 
    #[account(
        init, 
        payer = payer,
        seeds = [b"vault_mint".as_ref()], 
        bump, 
        mint::decimals = 9,
        mint::authority = pool_mint
    )] 
    pub pool_mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,

    // system stuff 
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub clearing_house_program: Program<'info, ClearingHouse>,
}


// #[derive(Accounts)]
// pub struct InitializeVault<'info> {
//     #[account(mut)]
//     pub payer: Signer<'info>, 
//     // #[account(mut, seeds=[b"authority"], bump)] 
//     pub authority: Signer<'info>,

//     // clearing house stuff 
//     pub clearing_house_state: AccountInfo<'info>,
//     #[account(mut)]
//     pub clearing_house_user: AccountInfo<'info>, // will be initialized by CPI?
//     #[account(mut)]
//     pub clearing_house_user_positions: Signer<'info>, // new KP? how to make PDA?
//     pub clearing_house_program: Program<'info, ClearingHouse>,

//     // pool mint for LPs 
//     #[account(
//         init, 
//         payer=payer,
//         seeds=[b"vault_mint"], 
//         bump, 
//         mint::decimals = 9,
//         mint::authority = pool_mint
//     )] 
//     pub pool_mint: Account<'info, Mint>,

//     // system stuff 
//     pub system_program: Program<'info, System>,
//     pub token_program: Program<'info, Token>,
//     pub rent: Sysvar<'info, Rent>,
// }