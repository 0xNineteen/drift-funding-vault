use anchor_lang::prelude::*;

use anchor_spl::{
    token::{
        Mint, Token, TokenAccount, 
    },
};
use clearing_house::context::{
    InitializeUserOptionalAccounts,
};
use clearing_house::cpi::accounts::{
    InitializeUserWithExplicitPayer,
};
use clearing_house::state::state::State;
use clearing_house::program::ClearingHouse;

use crate::state::VaultState;

pub fn initialize_vault(
    ctx: Context<InitializeVault>, 
    user_nonce: u8, 
    authority_nonce: u8,
    user_positions_nonce: u8,
) -> ProgramResult {

    // 1. create pool mint for LPs [done by anchor]
    // 2. create vault collateral ATA [done by anchor]

    let authority_seeds = [
        b"authority".as_ref(),
        &[authority_nonce][..],
    ];
    let user_positions_seeds = [
        b"user_positions".as_ref(),
        &[user_positions_nonce][..],
    ];
    let signers = &[&authority_seeds[..], &user_positions_seeds[..]];

    // 3. create drift account 
    let cpi_program = ctx.accounts.clearing_house_program.to_account_info();
    let cpi_accounts = InitializeUserWithExplicitPayer {
        state: ctx.accounts.state.to_account_info(), 
        user: ctx.accounts.user.to_account_info(), 
        user_positions: ctx.accounts.user_positions.clone(), 
        authority: ctx.accounts.authority.clone(), 

        payer: ctx.accounts.payer.clone(), 
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

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut, signer)]
    pub payer: AccountInfo<'info>,

    // drift account (user / user_positions will be initialized)
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    #[account(mut)]
    pub user: AccountInfo<'info>,
    #[account(mut, seeds = [b"user_positions".as_ref()], bump)]
    pub user_positions: AccountInfo<'info>,
    // drift clearing house 
    pub state: Box<Account<'info, State>>,
    
    // pool mint for LPs 
    #[account(
        init, 
        payer = payer,
        seeds = [b"vault_mint".as_ref()], 
        bump, 
        mint::decimals = 9,
        mint::authority = authority
    )] 
    pub vault_mint: Account<'info, Mint>,
    #[account(
        init, 
        payer = payer,
        seeds = [b"vault_state".as_ref()], 
        bump, 
    )] 
    pub vault_state: Account<'info, VaultState>,
    
    // vault collateral ATA
    #[account(
        init,
        payer = payer,
        seeds = [b"vault_collateral".as_ref()],
        bump,
        token::mint = collateral_mint,
        token::authority = authority
    )]
    pub vault_collateral: Box<Account<'info, TokenAccount>>,
    #[account(
        constraint = &state.collateral_mint.eq(&collateral_mint.key())
    )]
    pub collateral_mint: Box<Account<'info, Mint>>,

    // system stuff 
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clearing_house_program: Program<'info, ClearingHouse>,
}
