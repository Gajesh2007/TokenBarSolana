use anchor_lang::prelude::*;
use anchor_spl::token::{self, TokenAccount, Token, Mint};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod token_bar {
    use super::*;
    pub fn initialize(ctx: Context<Initialize>, pool_nonce: u8) -> ProgramResult {
        let token_bar = &mut ctx.accounts.token_bar;
        token_bar.token_mint = ctx.accounts.token_mint.key();
        token_bar.token_vault = ctx.accounts.token_vault.key();
        token_bar.nonce = pool_nonce;
        token_bar.xtoken_mint = ctx.accounts.xtoken_mint.key();

        Ok(())
    }

    pub fn enter(ctx: Context<Enter>, amount: u64) -> ProgramResult {
        if amount == 0 {
            return Err(ErrorCode::AmountMustBeGreaterThanZero.into());
        }

        let token_bar = &mut ctx.accounts.token_bar;

        // Gets the amount of tokens locked in the contract
        let total_tokens_locked = ctx.accounts.token_vault.amount; 
        // Gets the amount of xTokens in existence
        let total_shares = ctx.accounts.token_mint.supply;
        // If no xTokens exists, mint it 1:1 to the amount put in
        if total_shares == 0 || total_tokens_locked == 0 {
            {
                let seeds = &[token_bar.to_account_info().key.as_ref(), &[token_bar.nonce]];
                let pool_signer = &[&seeds[..]];

                let cpi_ctx = CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    token::MintTo {
                        mint: ctx.accounts.xtoken_mint.to_account_info(),
                        to: ctx.accounts.xtoken_vault.to_account_info(),
                        authority: ctx.accounts.pool_signer.to_account_info(),
                    },
                    pool_signer,
                );
                token::mint_to(cpi_ctx, amount)?;
            }
        } 
        // Calculate and mint the amount of xToken the Token is worth. The ratio will change overtime, as xToken is burned/minted and Token deposited + gained from fees / withdrawn.
        else {
            let what = amount.checked_mul(total_shares).unwrap().checked_div(total_tokens_locked).unwrap();
            {
                let seeds = &[token_bar.to_account_info().key.as_ref(), &[token_bar.nonce]];
                let pool_signer = &[&seeds[..]];

                let cpi_ctx = CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    token::MintTo {
                        mint: ctx.accounts.xtoken_mint.to_account_info(),
                        to: ctx.accounts.xtoken_vault.to_account_info(),
                        authority: ctx.accounts.pool_signer.to_account_info(),
                    },
                    pool_signer,
                );
                token::mint_to(cpi_ctx, what)?;
            }
        }

        // Lock the Tokens
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.stake_from_account.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(), 
                },
            );
            token::transfer(cpi_ctx, amount)?;
        }

        Ok(())
    }

    // Leave the token bar. Claim back your TOKENs.
    // Unlocks the staked + gained Token and burns xToken
    pub fn leave(ctx: Context<Leave>, share: u64) -> ProgramResult {
        if share == 0 {
            return Err(ErrorCode::AmountMustBeGreaterThanZero.into());
        }

        let token_bar = &mut ctx.accounts.token_bar;

        // Gets the amount of tokens locked in the contract
        let token_balance = ctx.accounts.token_vault.amount; 
        // Gets the amount of xTokens in existence
        let total_shares = ctx.accounts.token_mint.supply;
        // Calculates the amount of Token the xToken is worth
        let what = share.checked_mul(token_balance).unwrap().checked_div(total_shares).unwrap();
        // Burns Shares
        {
            let cpi_ctx = CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                token::Burn {
                    mint: ctx.accounts.xtoken_mint.to_account_info(),
                    to: ctx.accounts.xtoken_vault.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(), 
                },
            );
            token::burn(cpi_ctx, share)?;
        }
        // Transfer Tokens
        {
            let seeds = &[token_bar.to_account_info().key.as_ref(), &[token_bar.nonce]];
            let pool_signer = &[&seeds[..]];

            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.user_token_vault.to_account_info(),
                    authority: ctx.accounts.pool_signer.to_account_info(),
                },
                pool_signer,
            );
            token::transfer(cpi_ctx, what)?;
        }

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(pool_nonce: u8)]
pub struct Initialize<'info> {
    #[account(init, payer = signer)]
    pub token_bar: Account<'info, TokenBar>,
    #[account(mut)]
    pub signer: Signer<'info>,

    pub token_mint: Account<'info, Mint>,
    #[account(
        constraint = token_vault.mint == token_mint.key(),
        constraint = token_vault.owner == pool_signer.key(),
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = xtoken_mint.mint_authority == pool_signer.key().into(),
    )]
    pub xtoken_mint: Account<'info, Mint>,

    #[account(
        seeds = [
            token_bar.to_account_info().key.as_ref()
        ],
        bump = pool_nonce,
    )]
    pub pool_signer: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Enter<'info> {
    #[account(
        mut,
        has_one = token_vault
    )]
    pub token_bar: Account<'info, TokenBar>,
    pub user: Signer<'info>,

    #[account(
        seeds = [
            token_bar.to_account_info().key.as_ref()
        ],
        bump = token_bar.nonce,
    )]
    pub pool_signer: UncheckedAccount<'info>,

    pub token_mint: Account<'info, Mint>,
    #[account(
        constraint = token_vault.mint == token_mint.key(),
        constraint = token_vault.owner == pool_signer.key(),
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    stake_from_account: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = xtoken_mint.mint_authority == pool_signer.key().into(),
    )]
    pub xtoken_mint: Account<'info, Mint>,
    #[account(
        constraint = xtoken_vault.mint == xtoken_mint.key(),
        constraint = xtoken_vault.owner == user.key(),
    )]
    pub xtoken_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct Leave<'info> {
    #[account(
        mut,
        has_one = token_vault
    )]
    pub token_bar: Account<'info, TokenBar>,
    pub user: Signer<'info>,

    #[account(
        seeds = [
            token_bar.to_account_info().key.as_ref()
        ],
        bump = token_bar.nonce,
    )]
    pub pool_signer: UncheckedAccount<'info>,

    pub token_mint: Account<'info, Mint>,
    #[account(
        constraint = token_vault.mint == token_mint.key(),
        constraint = token_vault.owner == pool_signer.key(),
    )]
    pub token_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub user_token_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = xtoken_mint.mint_authority == pool_signer.key().into(),
    )]
    pub xtoken_mint: Account<'info, Mint>,
    #[account(
        constraint = xtoken_vault.mint == xtoken_mint.key(),
        constraint = xtoken_vault.owner == user.key(),
    )]
    pub xtoken_vault: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(Default)]
pub struct TokenBar {
    pub token_mint: Pubkey,
    pub token_vault: Pubkey,
    pub xtoken_mint: Pubkey,
    pub nonce: u8,
}

#[error]
pub enum ErrorCode {
    #[msg("Insufficient funds to unstake.")]
    InsufficientFundUnstake,
    #[msg("Amount must be greater than zero.")]
    AmountMustBeGreaterThanZero,
}