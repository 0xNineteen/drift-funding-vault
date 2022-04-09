use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount, mint_to, MintTo, Transfer, transfer, Burn, burn},
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
use clearing_house::math::casting::{cast, cast_to_i128};
use clearing_house::error::ErrorCode;

use clearing_house::controller::position::{get_position_index, add_new_position};
use clearing_house::math::position::calculate_base_asset_value_and_pnl;

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
        // 2 step process bc of auth: 
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
            
            // other
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

        let approx_funding;
        {
            let market = &ctx.accounts.markets.load()?
                .markets[Markets::index_from_u64(market_index)];
            let oracle_price_twap = market.amm.last_oracle_price_twap;
            let mark_price_twap = market.amm.last_mark_price_twap;
    
            // negative = shorts pay longs (should go long)
            // positive = longs pay shorts (should go short)
            approx_funding = cast_to_i128(mark_price_twap)?
                .checked_sub(oracle_price_twap)
                .ok_or_else(math_error!())?;
    
            msg!("(mark twap, oracle twap): {} {} approx funding: {}", mark_price_twap, oracle_price_twap, approx_funding);
        }

        if approx_funding == 0 { 
            msg!("funding = 0, not doing anything...");
            return Ok(());
        }
        
        // get current position (LONG, SHORT, or NONE)
        let vault_position = ctx.accounts.get_current_position(market_index);
        msg!("current position: {:?}", vault_position);
        
        // get authority signature 
        let authority_seeds = [
            b"authority".as_ref(),
            &[authority_nonce][..],
        ];
        let signers = &[&authority_seeds[..]];

        {
            let collateral_amount = &ctx.accounts.user.collateral;
            msg!("collateral amount: {}", collateral_amount);
        }
        
        // compute total collateral
        // amount_to_trade = collateral - liabilities > 0 
        // TODO: macro this bad boy probs
        let collateral_amount = ctx.accounts.user.collateral;
        let liabilites_amount = ctx.accounts.compute_total_liabilies();
        msg!("(collateral, liabilities) amount: {}, {}", collateral_amount, liabilites_amount);

        let amount_to_trade = if liabilites_amount > collateral_amount { 
            0 
        } else {
            collateral_amount - liabilites_amount
        };
        msg!("amount_to_trade: {}", amount_to_trade);

        /* For now, if we need to reverse we use 2 steps (close, new_pos) but 
         * in future we can do this in a single step for less fees 
         */

        if approx_funding < 0 { // funding goes to longs 
            
            if vault_position == Position::Short {
                // close long 
                msg!("closing short...");
                ctx.accounts.close_position(
                    signers, 
                    market_index
                )?;
                let user = &mut ctx.accounts.user; 
                user.reload()?; // update underlying account 
            }
            
            let collateral_amount = ctx.accounts.user.collateral;
            let liabilites_amount = ctx.accounts.compute_total_liabilies();
            msg!("(collateral, liabilities) amount: {}, {}", collateral_amount, liabilites_amount);
            
            let amount_to_trade = if liabilites_amount > collateral_amount { 
                0 
            } else {
                collateral_amount - liabilites_amount
            };
            msg!("amount_to_trade: {}", amount_to_trade);

            if amount_to_trade > 0 {
                msg!("opening long...");
                ctx.accounts.open_position(
                    amount_to_trade, 
                    0, 
                    ClearingHousePositionDirection::Long, 
                    signers, 
                    market_index
                )?;
            }

        } else { // funding goes to shorts 

            if vault_position == Position::Long {
                // close long 
                msg!("closing long...");
                ctx.accounts.close_position(
                    signers, 
                    market_index
                )?;
                let user = &mut ctx.accounts.user; 
                user.reload()?; // update underlying account 
            }

            let collateral_amount = ctx.accounts.user.collateral;
            let liabilites_amount = ctx.accounts.compute_total_liabilies();
            msg!("(collateral, liabilities) amount: {}, {}", collateral_amount, liabilites_amount);
            let amount_to_trade = if liabilites_amount > collateral_amount { 
                0 
            } else {
                collateral_amount - liabilites_amount
            };
            msg!("amount_to_trade: {}", amount_to_trade);

            if amount_to_trade > 0 {
                msg!("opening short...");
                ctx.accounts.open_position(
                    amount_to_trade, 
                    0, 
                    ClearingHousePositionDirection::Short, 
                    signers, 
                    market_index
                )?;
            }
        }

        Ok(())
    }

}

#[derive(Debug, PartialEq)]
pub enum Position { 
    Long, 
    Short, 
    None
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
    pub clearing_house_markets: AccountInfo<'info>,
    #[account(mut)]
    pub clearing_house_funding_payment_history: AccountInfo<'info>,
    #[account(mut)]
    pub clearing_house_deposit_history: AccountInfo<'info>,

    // programs 
    pub clearing_house_program: Program<'info, ClearingHouse>,
    pub token_program: Program<'info, Token>,
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

#[derive(Accounts)]
pub struct UpdatePosition<'info> {
    #[account(mut, seeds = [b"authority".as_ref()], bump)]
    pub authority: AccountInfo<'info>,
    #[account(mut, seeds = [b"user_positions".as_ref()], bump)]
    pub user_positions: AccountLoader<'info, UserPositions>,

    #[account(mut)]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub user: Box<Account<'info, User>>,
    #[account(mut)]
    pub markets: AccountLoader<'info, Markets>,
    #[account(mut)]
    pub trade_history: AccountInfo<'info>,
    #[account(mut)]
    pub funding_payment_history: AccountInfo<'info>,
    #[account(mut)]
    pub funding_rate_history: AccountInfo<'info>,
    pub oracle: AccountInfo<'info>,
    pub clearing_house_program: Program<'info, ClearingHouse>,
}

impl<'info> UpdatePosition<'info> {

    fn get_current_position(
        &self, 
        market_index: u64,
    ) -> Position {
        let vault_positions = &self.user_positions.load().unwrap();
        let position_index = vault_positions
            .positions
            .iter()
            .position(|market_position| market_position.is_for(market_index));

        match position_index {
            None => Position::None, 
            Some(position_index) => {
                let vault_market_position = &vault_positions.positions[position_index];
                if vault_market_position.base_asset_amount > 0 { 
                    Position::Long
                } else { 
                    Position::Short
                }
            }   
        }
    }

    fn compute_total_liabilies(
        &self
    ) -> u128 {
        let vault_positions = &self.user_positions.load().unwrap();
        let markets = &self.markets.load().unwrap();
        
        let mut liabilites_amount = 0; 
        for market_position in vault_positions.positions.iter() {
            if market_position.base_asset_amount == 0 {
                continue;
            }
            let market = markets.get_market(market_position.market_index);
            let amm = &market.amm;
            let (position_base_asset_value, _position_unrealized_pnl) =
                calculate_base_asset_value_and_pnl(market_position, amm).unwrap();
            
            liabilites_amount += position_base_asset_value;
        }

        liabilites_amount
    }

    fn close_position(
        &self, 
        signers: &[&[&[u8]]],
        market_index: u64,
    ) -> ProgramResult {

        let cpi_program = self.clearing_house_program.to_account_info();
        let cpi_accounts = ClearingHouseClosePosition {
            state: self.state.to_account_info(),
            user: self.user.to_account_info(),
            user_positions: self.user_positions.to_account_info(),
            authority: self.authority.clone(),
            markets: self.markets.to_account_info(),
            oracle: self.oracle.clone(),
            trade_history: self.trade_history.to_account_info(),
            funding_payment_history: self.funding_payment_history.to_account_info(),
            funding_rate_history: self.funding_rate_history.to_account_info(),
        };
                
        let cpi_ctx = CpiContext::new_with_signer(
            cpi_program, 
            cpi_accounts,
            signers
        );
        clearing_house::cpi::close_position(
            cpi_ctx,
            market_index,
            ClearingHouseManagePositionOptionalAccounts {
                discount_token: false,
                referrer: false,
            },
        )
    }
 
    fn open_position(
        &self,
        amount_in: u128,
        limit_price: u128, 
        position_direction: ClearingHousePositionDirection, 
        signers: &[&[&[u8]]],
        market_index: u64,
    ) -> ProgramResult {
        let cpi_program = self.clearing_house_program.to_account_info();
        let cpi_accounts = ClearingHouseOpenPosition {
            state: self.state.to_account_info(),
            user: self.user.to_account_info(),
            user_positions: self.user_positions.to_account_info(),
            authority: self.authority.clone(),
            markets: self.markets.to_account_info(),
            oracle: self.oracle.clone(),
            trade_history: self.trade_history.to_account_info(),
            funding_payment_history: self.funding_payment_history.to_account_info(),
            funding_rate_history: self.funding_rate_history.to_account_info(),
        };
        
        let cpi_ctx = CpiContext::new_with_signer(
            cpi_program, 
            cpi_accounts,
            signers
        );

        clearing_house::cpi::open_position(
            cpi_ctx, 
            position_direction,
            amount_in, 
            market_index, 
            limit_price, 
            ClearingHouseManagePositionOptionalAccounts {
                discount_token: false,
                referrer: false,
            },
        )
    }
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

#[error]
pub enum VaultErrorCode {
    #[msg("Not enough funds.")]
    NotEnoughFunds,
    #[msg("Widthdraw amount too small.")]
    WidthdrawAmountTooSmall,
}