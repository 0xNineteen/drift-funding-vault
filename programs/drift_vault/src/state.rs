use anchor_lang::prelude::*;

#[account]
#[derive(Default)]
pub struct VaultState {
    pub total_amount_minted: u64, 
}

#[derive(Debug, PartialEq)]
pub enum Position { 
    Long, 
    Short, 
    None
}