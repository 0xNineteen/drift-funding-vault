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
