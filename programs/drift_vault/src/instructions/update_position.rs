use anchor_lang::prelude::*;

use clearing_house::context::{
    ManagePositionOptionalAccounts as ClearingHouseManagePositionOptionalAccounts,
};
use clearing_house::controller::position::PositionDirection as ClearingHousePositionDirection;
use clearing_house::cpi::accounts::{
    ClosePosition as ClearingHouseClosePosition,
    OpenPosition as ClearingHouseOpenPosition,
};
use clearing_house::state::state::State;
use clearing_house::program::ClearingHouse;
use clearing_house::state::{
    market::Markets,
    user::{User, UserPositions},
};
use clearing_house::math::casting::{cast_to_i128};
use clearing_house::error::ErrorCode;
use clearing_house::math::position::calculate_base_asset_value_and_pnl;

use crate::state::{Position};
use crate::math_error;

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

    pub fn get_current_position(
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

    pub fn compute_total_liabilies(
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

    pub fn close_position(
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
 
    pub fn open_position(
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
