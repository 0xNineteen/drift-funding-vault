use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, mint_to, MintTo, Transfer, transfer},
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
        ctx: Context<InitializeVault>, 
        user_nonce: u8, 
        authority_nonce: u8,
        user_positions_nonce: u8,
    ) -> ProgramResult {

        // create pool mint for LPs [done by anchor]
        // create vault collateral ATA [done by anchor]

        // create drift account 
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
    pub fn deposit(
        ctx: Context<Deposit>, 
        deposit_amount: u64,
        authority_nonce: u8,
    ) -> ProgramResult {
        let vault_state = &mut ctx.accounts.vault_state;

        // mint = same amount as USDC deposited
        let mint_amount = deposit_amount; 

        // record deposit 
        vault_state.total_amount_minted = 
            vault_state.total_amount_minted
            .checked_add(mint_amount)
            .unwrap();
        
        // send mint to user 
        let auth_seeds = [
            b"authority".as_ref(),
            &[authority_nonce][..],
        ];
        let signers = &[&auth_seeds[..]];
        let mint_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(), 
            MintTo {
                to: ctx.accounts.user_vault_ata.to_account_info(),
                mint: ctx.accounts.vault_mint.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            }
        );
        mint_to(
            mint_ctx.with_signer(signers), 
            mint_amount
        )?;

        // deposit usdc to vault's drift collateral 
        // 2 step process: [depositer => {vault collateral] => drift account} :(

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
        let authority_seeds = [
            b"authority".as_ref(),
            &[authority_nonce][..],
        ];
        let signers = &[&authority_seeds[..]];

        let cpi_program = ctx.accounts.clearing_house_program.to_account_info();
        let cpi_accounts = ClearingHouseDepositCollateral {
            // user stuff 
            user: ctx.accounts.clearing_house_user.to_account_info(), // PDA
            user_collateral_account: ctx.accounts.vault_collateral_ata.to_account_info(), // [!]
            user_positions: ctx.accounts.clearing_house_user_positions.to_account_info(),// KP
            authority: ctx.accounts.authority.clone(), // KP 

            // drift stuff 
            state: ctx.accounts.clearing_house_state.to_account_info(), // CH 
            markets: ctx.accounts.clearing_house_markets.to_account_info(), // CH 
            collateral_vault: ctx.accounts.clearing_house_collateral_vault.to_account_info(), // CH 
            deposit_history: ctx.accounts.clearing_house_deposit_history.to_account_info(),// CH 
            funding_payment_history: ctx.accounts.clearing_house_funding_payment_history.to_account_info(), // CH 
            token_program: ctx.accounts.token_program.to_account_info(), // basic
        };
        let cpi_ctx = CpiContext::new_with_signer(
            cpi_program, 
            cpi_accounts,
            signers
        );
        clearing_house::cpi::deposit_collateral(cpi_ctx, deposit_amount)?;

        Ok(())
    }

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
pub struct InitializeVault<'info> {
    #[account(mut, signer)]
    pub payer: AccountInfo<'info>,

    // drift account 
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    pub clearing_house_state: Box<Account<'info, State>>,
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
        constraint = &clearing_house_state.collateral_mint.eq(&collateral_mint.key())
    )]
    pub collateral_mint: Box<Account<'info, Mint>>,

    // system stuff 
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub clearing_house_program: Program<'info, ClearingHouse>,
}


#[account]
#[derive(Default)]
pub struct VaultState {
    pub total_amount_minted: u64, 
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(signer)]
    pub owner: AccountInfo<'info>, // depositer / owner of ATAs 

    // atas 
        // vault 
    #[account(mut, constraint = &vault_collateral_ata.mint.eq(&clearing_house_state.collateral_mint))]
    pub vault_collateral_ata: Box<Account<'info, TokenAccount>>,  // transfer: this => assoc. drift collateral account
        // user
    #[account(mut, constraint = &user_collateral_ata.mint.eq(&clearing_house_state.collateral_mint))]
    pub user_collateral_ata: Box<Account<'info, TokenAccount>>, // transfer: this => vault collateral ata
    #[account(mut, has_one = owner, constraint = &user_vault_ata.mint.eq(&vault_mint.key()))]
    pub user_vault_ata: Box<Account<'info, TokenAccount>>,  // mint to this 
    
    // vault stuff 
    #[account(
        mut,
        seeds = [b"vault_mint".as_ref()], 
        bump, 
    )] 
    pub vault_mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"vault_state".as_ref()], 
        bump, 
    )] 
    pub vault_state: Account<'info, VaultState>,
    
    // vault drift stuff
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    #[account(mut, seeds = [b"user_positions".as_ref()], bump)]
    pub clearing_house_user_positions: AccountInfo<'info>,
    #[account(mut)]
    pub clearing_house_user: AccountInfo<'info>,

    // drift clearing house stuff 
    #[account(mut)]
    pub clearing_house_state: Box<Account<'info, State>>,
    #[account(mut)]
    pub clearing_house_collateral_vault: Box<Account<'info, TokenAccount>>,
    pub clearing_house_markets: AccountLoader<'info, Markets>,
    #[account(mut)]
    pub clearing_house_funding_payment_history: AccountLoader<'info, FundingPaymentHistory>,
    #[account(mut)]
    pub clearing_house_deposit_history: AccountLoader<'info, DepositHistory>,

    // programs 
    pub clearing_house_program: Program<'info, ClearingHouse>,
    pub token_program: Program<'info, Token>,
}
