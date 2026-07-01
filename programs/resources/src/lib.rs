// programs/resources/src/lib.rs
//
// Generic controller for fungible game resources (Token-2022). The FIRST
// resource is SOUL — combat's reward currency and the fuel for item awakening.
// The same program manages any number of resource mints (crafting mats, event
// currencies) — one ResourceController PDA per mint, each with its own capped
// faucet and burn sink. Mirrors the native-token discipline in game_core: a
// player can NEVER mint; only the authorized backend can, under a per-epoch cap.
//
// Cargo: anchor-lang = "0.31", anchor-spl = "0.31".

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::instruction::AuthorityType;
use anchor_spl::token_interface::{
    self, Burn, Mint, MintTo, SetAuthority, TokenAccount, TokenInterface,
};

declare_id!("ReSo1111111111111111111111111111111111111111"); // placeholder

#[program]
pub mod game_resources {
    use super::*;

    /// Registers a resource mint. The mint must currently have `admin` as its
    /// mint authority; this hands that authority to the per-mint controller PDA.
    pub fn init_resource(ctx: Context<InitResource>, params: InitResourceParams) -> Result<()> {
        let r = &mut ctx.accounts.controller;
        r.admin = ctx.accounts.admin.key();
        r.authorized_minter = params.authorized_minter;
        r.mint = ctx.accounts.mint.key();
        r.cap_per_epoch = params.cap_per_epoch;
        r.minted_this_epoch = 0;
        r.epoch = 0;
        r.paused = false;
        r.bump = ctx.bumps.controller;

        token_interface::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                SetAuthority {
                    current_authority: ctx.accounts.admin.to_account_info(),
                    account_or_mint: ctx.accounts.mint.to_account_info(),
                },
            ),
            AuthorityType::MintTokens,
            Some(r.key()),
        )?;

        emit!(ResourceInitialized { mint: r.mint, cap_per_epoch: r.cap_per_epoch });
        Ok(())
    }

    /// FAUCET. Combat reward / drop. Backend-only, per-epoch capped.
    pub fn mint_resource(ctx: Context<MintResource>, amount: u64) -> Result<()> {
        let r = &mut ctx.accounts.controller;
        require!(!r.paused, ResourceError::Paused);
        require_keys_eq!(ctx.accounts.minter.key(), r.authorized_minter, ResourceError::Unauthorized);

        let projected = r.minted_this_epoch.checked_add(amount).ok_or(ResourceError::MathOverflow)?;
        require!(projected <= r.cap_per_epoch, ResourceError::EpochCapExceeded);

        let mint_key = r.mint;
        let signer_seeds: &[&[&[u8]]] = &[&[b"resource", mint_key.as_ref(), &[r.bump]]];
        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.recipient_token_account.to_account_info(),
                    authority: r.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
        )?;

        r.minted_this_epoch = projected;
        emit!(ResourceMinted { mint: mint_key, amount, epoch: r.epoch });
        Ok(())
    }

    pub fn advance_epoch(ctx: Context<AdminOnly>) -> Result<()> {
        let r = &mut ctx.accounts.controller;
        r.epoch = r.epoch.checked_add(1).ok_or(ResourceError::MathOverflow)?;
        r.minted_this_epoch = 0;
        Ok(())
    }

    /// SINK. Burns resource (awakening cost, crafting, etc.). Callable directly
    /// by the owner or via CPI from awakening/crafting programs.
    pub fn burn_resource(ctx: Context<BurnResource>, amount: u64) -> Result<()> {
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.from.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            amount,
        )?;
        emit!(ResourceBurned { mint: ctx.accounts.mint.key(), amount });
        Ok(())
    }

    pub fn update_resource(ctx: Context<AdminOnly>, params: UpdateResourceParams) -> Result<()> {
        let r = &mut ctx.accounts.controller;
        if let Some(v) = params.authorized_minter { r.authorized_minter = v; }
        if let Some(v) = params.cap_per_epoch { r.cap_per_epoch = v; }
        if let Some(v) = params.paused { r.paused = v; }
        Ok(())
    }
}

// ----------------------------- accounts -----------------------------

#[derive(Accounts)]
pub struct InitResource<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    /// Must currently have `admin` as mint authority.
    #[account(mut)] pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init, payer = admin, space = 8 + ResourceController::INIT_SPACE,
        seeds = [b"resource", mint.key().as_ref()], bump
    )]
    pub controller: Account<'info, ResourceController>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintResource<'info> {
    pub minter: Signer<'info>,
    #[account(mut, seeds = [b"resource", mint.key().as_ref()], bump = controller.bump)]
    pub controller: Account<'info, ResourceController>,
    #[account(mut, address = controller.mint)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct BurnResource<'info> {
    pub owner: Signer<'info>,
    #[account(seeds = [b"resource", mint.key().as_ref()], bump = controller.bump)]
    pub controller: Account<'info, ResourceController>,
    #[account(mut, address = controller.mint)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub from: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"resource", controller.mint.as_ref()], bump = controller.bump, has_one = admin @ ResourceError::Unauthorized)]
    pub controller: Account<'info, ResourceController>,
}

// ------------------------------ state -------------------------------

#[account]
#[derive(InitSpace)]
pub struct ResourceController {
    pub admin: Pubkey,
    pub authorized_minter: Pubkey,
    pub mint: Pubkey,
    pub cap_per_epoch: u64,
    pub minted_this_epoch: u64,
    pub epoch: u64,
    pub paused: bool,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitResourceParams {
    pub authorized_minter: Pubkey,
    pub cap_per_epoch: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateResourceParams {
    pub authorized_minter: Option<Pubkey>,
    pub cap_per_epoch: Option<u64>,
    pub paused: Option<bool>,
}

#[event] pub struct ResourceInitialized { pub mint: Pubkey, pub cap_per_epoch: u64 }
#[event] pub struct ResourceMinted { pub mint: Pubkey, pub amount: u64, pub epoch: u64 }
#[event] pub struct ResourceBurned { pub mint: Pubkey, pub amount: u64 }

#[error_code]
pub enum ResourceError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Per-epoch cap exceeded")] EpochCapExceeded,
    #[msg("Minting is paused")] Paused,
    #[msg("Math overflow")] MathOverflow,
}
