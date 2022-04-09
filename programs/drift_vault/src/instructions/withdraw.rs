use anchor_lang::prelude::*;

use anchor_spl::{
    token::{
        Mint, Token, TokenAccount, 
        Transfer, transfer, 
        Burn, burn
    },
};

use clearing_house::controller::position::PositionDirection as ClearingHousePositionDirection;
use clearing_house::cpi::accounts::{
    WithdrawCollateral as ClearingHouseWithdrawCollateral,
};

use crate::state::{VaultState, Position};
use crate::error::VaultErrorCode;
use crate::instructions::update_position::*;

pub fn withdraw(
    ctx: Context<Withdraw>, 
    burn_amount: u128,
    market_index: u64,
    authority_nonce: u8,
) -> ProgramResult {
    let update_position_accounts = &mut ctx.accounts.update_position;

    // compute total amount of collateral 
    let collateral_amount = update_position_accounts.user.collateral;
    let liabilites_amount = update_position_accounts.compute_total_liabilies();
    msg!("(collateral, liabilities) amount: {}, {}", collateral_amount, liabilites_amount);

    let user_vault_balance = ctx.accounts.user_vault_ata.amount as u128; 
    require!(user_vault_balance >= burn_amount, VaultErrorCode::NotEnoughFunds);

    let state = &mut ctx.accounts.vault_state; 
    // collateral to give = (burn_amount / total_minted) * total_colateral
    let mut refund_collateral_amount = 
        burn_amount
            .checked_mul(collateral_amount).unwrap()
            .checked_div(state.total_amount_minted as u128).unwrap() as u64;
    msg!("estimated refund amount: {}", refund_collateral_amount);
    require!(refund_collateral_amount > 0, VaultErrorCode::WidthdrawAmountTooSmall);

    let authority_seeds = [
        b"authority".as_ref(),
        &[authority_nonce][..],
    ];
    let signers = &[&authority_seeds[..]];

    // reduce position (want approx 1:1 collat:liabilities)
    let new_collateral_amount = collateral_amount - refund_collateral_amount as u128;
    let amount_to_reduce = liabilites_amount - new_collateral_amount; 
    let vault_position = update_position_accounts.get_current_position(market_index);
    msg!("vaults current position: {:?}", vault_position);

    if vault_position != Position::None {
        let reduce_direction = match vault_position {
            Position::Long => {
                msg!("position reducing: {:?} {:?}", Position::Short, amount_to_reduce);
                ClearingHousePositionDirection::Short
            },
            Position::Short => {
                msg!("position reducing: {:?} {:?}", Position::Long, amount_to_reduce);
                ClearingHousePositionDirection::Long
            },
            _ => panic!("shouldnt be called...")
        };
    
        update_position_accounts.open_position(
            amount_to_reduce, 
            0, 
            reduce_direction, 
            signers,
            market_index,
        )?;

        // compute total amount of collateral 
        update_position_accounts.user.reload()?;
        let collateral_amount = update_position_accounts.user.collateral;
        let liabilites_amount = update_position_accounts.compute_total_liabilies();
        msg!("(collateral, liabilities) amount: {}, {}", collateral_amount, liabilites_amount);        

        // re-compute refund amount after close (collateral estimate isnt perfect bc slippage + fees)
        refund_collateral_amount = 
            burn_amount
            .checked_mul(collateral_amount).unwrap()
            .checked_div(state.total_amount_minted as u128).unwrap() as u64;
        msg!("estimated refund amount: {}", refund_collateral_amount);
        require!(refund_collateral_amount > 0, VaultErrorCode::WidthdrawAmountTooSmall);
    }

    // withdraw + transfer to user 
    // (1) drift => vault collateral
    let cpi_program = update_position_accounts.clearing_house_program.to_account_info();
    let cpi_accounts = ClearingHouseWithdrawCollateral {
        // user stuff 
        user: update_position_accounts.user.to_account_info(), // PDA
        user_collateral_account: ctx.accounts.vault_collateral_ata.to_account_info(), // [!]
        user_positions: update_position_accounts.user_positions.to_account_info(),// KP
        authority: update_position_accounts.authority.clone(), // KP 

        // drift stuff 
        state: update_position_accounts.state.to_account_info(), // CH 
        markets: update_position_accounts.markets.to_account_info(), // CH 
        collateral_vault: ctx.accounts.collateral_vault.to_account_info(), // CH 
        deposit_history: ctx.accounts.deposit_history.to_account_info(),// CH 
        funding_payment_history: update_position_accounts.funding_payment_history.to_account_info(), // CH 

        collateral_vault_authority: ctx.accounts.collateral_vault_authority.to_account_info(),// CH 
        insurance_vault: ctx.accounts.insurance_vault.to_account_info(),// CH 
        insurance_vault_authority: ctx.accounts.insurance_vault_authority.to_account_info(),// CH 
        
        // other
        token_program: ctx.accounts.token_program.to_account_info(), // basic
    };
    let cpi_ctx = CpiContext::new_with_signer(
        cpi_program, 
        cpi_accounts,
        signers
    );
    clearing_house::cpi::withdraw_collateral(cpi_ctx, refund_collateral_amount)?;

    // sanity check (make sure we got the correct output)
    let balance_before = ctx.accounts.vault_collateral_ata.amount;
    ctx.accounts.vault_collateral_ata.reload()?; // update underlying 
    let balance_after = ctx.accounts.vault_collateral_ata.amount;
    require!(balance_after - balance_before == refund_collateral_amount, 
        VaultErrorCode::WidthdrawAmountTooSmall); 

    // (2) vault collateral => user ata 
    transfer(CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.vault_collateral_ata.to_account_info(),
            to: ctx.accounts.user_collateral_ata.to_account_info(),
            authority: update_position_accounts.authority.to_account_info(),
        }
    ).with_signer(signers), refund_collateral_amount)?;

    // burn pool tokens 
    burn(CpiContext::new(
        ctx.accounts.token_program.to_account_info(), 
    Burn { 
            mint: ctx.accounts.vault_mint.to_account_info(), 
            to: ctx.accounts.user_vault_ata.to_account_info(), 
            authority: ctx.accounts.owner.to_account_info(),
        }
    ), burn_amount as u64)?;
    
    // update state 
    state.total_amount_minted = state.total_amount_minted
        .checked_sub(burn_amount as u64).unwrap(); 

    Ok(())
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(signer)]
    pub owner: AccountInfo<'info>, // depositer / owner of ATAs 

    // atas 
        // vault 
    #[account(mut, constraint = &vault_collateral_ata.mint.eq(&update_position.state.collateral_mint))]
    pub vault_collateral_ata: Box<Account<'info, TokenAccount>>,  
        // user
    #[account(mut, constraint = &user_collateral_ata.mint.eq(&update_position.state.collateral_mint))]
    pub user_collateral_ata: Box<Account<'info, TokenAccount>>, 
    #[account(mut, has_one = owner, constraint = &user_vault_ata.mint.eq(&vault_mint.key()))]
    pub user_vault_ata: Box<Account<'info, TokenAccount>>,  

    // vault stuff 
    #[account(mut, seeds = [b"vault_state".as_ref()], bump)] 
    pub vault_state: Account<'info, VaultState>,
    #[account(mut, seeds = [b"vault_mint".as_ref()], bump)] 
    pub vault_mint: Account<'info, Mint>,

    // additional drift things 
    #[account(mut)]
    pub collateral_vault: Box<Account<'info, TokenAccount>>,
    pub collateral_vault_authority: AccountInfo<'info>,
    #[account(mut)]
    pub deposit_history: AccountInfo<'info>,
    #[account(mut)]
    pub insurance_vault: Box<Account<'info, TokenAccount>>,
    pub insurance_vault_authority: AccountInfo<'info>,

    pub update_position: UpdatePosition<'info>, // lots of drift things 

    // other
    pub token_program: Program<'info, Token>,
}
