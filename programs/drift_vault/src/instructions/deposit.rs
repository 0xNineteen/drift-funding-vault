use anchor_lang::prelude::*;

use anchor_spl::{
    token::{
        Mint, Token, TokenAccount, 
        MintTo, mint_to, 
        Transfer, transfer, 
    },
};

use clearing_house::cpi::accounts::{
    DepositCollateral as ClearingHouseDepositCollateral, 
};
use clearing_house::state::state::State;
use clearing_house::program::ClearingHouse;

use crate::state::VaultState;

pub fn deposit(
    ctx: Context<Deposit>, 
    deposit_amount: u64,
    authority_nonce: u8,
) -> ProgramResult {
    let vault_state = &mut ctx.accounts.vault_state;

    // 1. mint pool tokens to user
    // mint amount = same amount as USDC deposited
    let mint_amount = deposit_amount; 

    // record deposit in state 
    vault_state.total_amount_minted = vault_state.total_amount_minted
        .checked_add(mint_amount)
        .unwrap();
    
    let authority_seeds = [
        b"authority".as_ref(),
        &[authority_nonce][..],
    ];
    let signers = &[&authority_seeds[..]];
    
    // send mint to user 
    mint_to(CpiContext::new(
        ctx.accounts.token_program.to_account_info(), 
        MintTo {
            to: ctx.accounts.user_vault_ata.to_account_info(),
            mint: ctx.accounts.vault_mint.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        }).with_signer(signers), 
        mint_amount
    )?;

    // 2. deposit usdc to vault's drift collateral 
    // two step process bc of auth: 
    // [depositer => {vault collateral] => drift account}

    // (1) depositer => vault collateral 
    transfer(CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.user_collateral_ata.to_account_info(),
            to: ctx.accounts.vault_collateral_ata.to_account_info(),
            authority: ctx.accounts.owner.to_account_info(),
        }
    ), deposit_amount)?;
    
    // (2) vault collateral => drift 
    let cpi_program = ctx.accounts.clearing_house_program.to_account_info();
    let cpi_accounts = ClearingHouseDepositCollateral {
        // user stuff 
        user: ctx.accounts.user.to_account_info(), 
        user_collateral_account: ctx.accounts.vault_collateral_ata.to_account_info(), // [!]
        user_positions: ctx.accounts.user_positions.to_account_info(),
        authority: ctx.accounts.authority.clone(), 

        // drift stuff 
        state: ctx.accounts.state.to_account_info(),
        markets: ctx.accounts.markets.to_account_info(), 
        collateral_vault: ctx.accounts.collateral_vault.to_account_info(), 
        deposit_history: ctx.accounts.deposit_history.to_account_info(),
        funding_payment_history: ctx.accounts.funding_payment_history.to_account_info(), 
        
        // other
        token_program: ctx.accounts.token_program.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        cpi_program, 
        cpi_accounts,
        signers
    );
    clearing_house::cpi::deposit_collateral(cpi_ctx, deposit_amount)?;

    Ok(())
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    // depositer / owner of ATAs 
    #[account(signer)]
    pub owner: AccountInfo<'info>, 

    // ATAs 
        // vault 
    #[account(
        mut, 
        seeds = [b"vault_collateral".as_ref()],
        bump,
        constraint = &vault_collateral_ata.mint.eq(&state.collateral_mint)
    )]
    pub vault_collateral_ata: Box<Account<'info, TokenAccount>>,  // transfer: this => assoc. drift collateral account
        // user
    #[account(
        mut, 
        has_one = owner, 
        constraint = &user_collateral_ata.mint.eq(&state.collateral_mint)
    )]
    pub user_collateral_ata: Box<Account<'info, TokenAccount>>, // transfer: this => vault collateral ata
    #[account(
        mut, 
        has_one = owner, 
        constraint = &user_vault_ata.mint.eq(&vault_mint.key())
    )]
    pub user_vault_ata: Box<Account<'info, TokenAccount>>,  // mint to this 
    
    // vault stuff 
    #[account(mut, seeds = [b"vault_state".as_ref()], bump)] 
    pub vault_state: Account<'info, VaultState>,
    #[account(mut, seeds = [b"vault_mint".as_ref()], bump)] 
    pub vault_mint: Account<'info, Mint>,
    
    // drift vault stuff
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    #[account(mut, seeds = [b"user_positions".as_ref()], bump)]
    pub user_positions: AccountInfo<'info>,
    #[account(mut)]
    pub user: AccountInfo<'info>,

    // drift clearing house stuff 
    #[account(mut)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    pub markets: AccountInfo<'info>,
    #[account(mut)]
    pub funding_payment_history: AccountInfo<'info>,
    #[account(mut)]
    pub deposit_history: AccountInfo<'info>,

    // other
    pub clearing_house_program: Program<'info, ClearingHouse>,
    pub token_program: Program<'info, Token>,
}
