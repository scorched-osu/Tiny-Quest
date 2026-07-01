// programs/core/src/lib.rs
//
// Economic backbone for the game. Owns the native SPL/Token-2022 mint authority
// (held by the Config PDA), controls the reward "faucet" with a per-epoch hard
// cap, and exposes a burn "sink". This is what keeps the token economy from
// hyperinflating: players can NEVER mint. Only an off-chain authorized backend
// signer can call mint_reward, and only up to reward_cap_per_epoch each epoch.
//
// Anchor 0.30+/0.31, uses token_interface so the same code works for both the
// classic SPL Token program and Token-2022.

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::instruction::AuthorityType;
use anchor_spl::token_interface::{
    self, Burn, Mint, MintTo, SetAuthority, TokenAccount, TokenInterface,
};

declare_id!("23AMrskYWpeEQny9mwLsBW1j5sEu2FuJwDBQU9MeM463"); // placeholder — replace after `anchor keys list`

const MAX_BPS: u16 = 10_000;

#[program]
pub mod game_core {
    use super::*;

    /// One-time global setup. The `token_mint` must already exist with its mint
    /// authority currently set to `admin`. This instruction atomically hands that
    /// mint authority over to the Config PDA so the program controls emissions.
    pub fn initialize(ctx: Context<Initialize>, params: InitializeParams) -> Result<()> {
        require!(params.marketplace_fee_bps <= MAX_BPS, CoreError::InvalidBps);
        require!(params.royalty_bps <= MAX_BPS, CoreError::InvalidBps);
        require!(params.burn_share_bps <= MAX_BPS, CoreError::InvalidBps);

        let config = &mut ctx.accounts.config;
        config.admin = ctx.accounts.admin.key();
        config.authorized_minter = params.authorized_minter;
        config.token_mint = ctx.accounts.token_mint.key();
        config.treasury = ctx.accounts.treasury.key();
        config.marketplace_fee_bps = params.marketplace_fee_bps;
        config.royalty_bps = params.royalty_bps;
        config.burn_share_bps = params.burn_share_bps;
        config.reward_cap_per_epoch = params.reward_cap_per_epoch;
        config.minted_this_epoch = 0;
        config.epoch = 0;
        config.paused = false;
        config.bump = ctx.bumps.config;

        // Transfer mint authority: admin -> Config PDA.
        token_interface::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                SetAuthority {
                    current_authority: ctx.accounts.admin.to_account_info(),
                    account_or_mint: ctx.accounts.token_mint.to_account_info(),
                },
            ),
            AuthorityType::MintTokens,
            Some(config.key()),
        )?;

        emit!(Initialized {
            admin: config.admin,
            token_mint: config.token_mint,
        });
        Ok(())
    }

    /// FAUCET. Mints reward tokens to a player. Callable only by the backend's
    /// `authorized_minter` keypair (it signs after verifying gameplay off-chain),
    /// and only up to the per-epoch cap. This is the inflation valve.
    pub fn mint_reward(ctx: Context<MintReward>, amount: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require!(!config.paused, CoreError::Paused);
        require_keys_eq!(
            ctx.accounts.minter.key(),
            config.authorized_minter,
            CoreError::Unauthorized
        );

        let projected = config
            .minted_this_epoch
            .checked_add(amount)
            .ok_or(CoreError::MathOverflow)?;
        require!(
            projected <= config.reward_cap_per_epoch,
            CoreError::EpochCapExceeded
        );

        let signer_seeds: &[&[&[u8]]] = &[&[b"config", &[config.bump]]];
        token_interface::mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    to: ctx.accounts.recipient_token_account.to_account_info(),
                    authority: config.to_account_info(),
                },
                signer_seeds,
            ),
            amount,
        )?;

        config.minted_this_epoch = projected;
        emit!(RewardMinted {
            recipient: ctx.accounts.recipient_token_account.key(),
            amount,
            epoch: config.epoch,
        });
        Ok(())
    }

    /// Rolls to the next epoch and resets the per-epoch minted counter.
    /// Call this on your reward cadence (daily/weekly) from a cron/keeper.
    pub fn advance_epoch(ctx: Context<AdminOnly>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.epoch = config.epoch.checked_add(1).ok_or(CoreError::MathOverflow)?;
        config.minted_this_epoch = 0;
        emit!(EpochAdvanced { epoch: config.epoch });
        Ok(())
    }

    /// SINK. Burns tokens from the owner's account. Other programs (upgrades,
    /// crafting, repair) CPI into this — and it's also callable directly.
    pub fn burn_sink(ctx: Context<BurnSink>, amount: u64) -> Result<()> {
        token_interface::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.token_mint.to_account_info(),
                    from: ctx.accounts.from.to_account_info(),
                    authority: ctx.accounts.owner.to_account_info(),
                },
            ),
            amount,
        )?;
        emit!(Burned { amount });
        Ok(())
    }

    /// Admin knobs: rotate the backend minter key, retune fees/royalties/burn
    /// share, adjust the emission cap, or pause minting in an emergency.
    pub fn update_config(ctx: Context<AdminOnly>, params: UpdateConfigParams) -> Result<()> {
        let config = &mut ctx.accounts.config;
        if let Some(v) = params.authorized_minter {
            config.authorized_minter = v;
        }
        if let Some(v) = params.marketplace_fee_bps {
            require!(v <= MAX_BPS, CoreError::InvalidBps);
            config.marketplace_fee_bps = v;
        }
        if let Some(v) = params.royalty_bps {
            require!(v <= MAX_BPS, CoreError::InvalidBps);
            config.royalty_bps = v;
        }
        if let Some(v) = params.burn_share_bps {
            require!(v <= MAX_BPS, CoreError::InvalidBps);
            config.burn_share_bps = v;
        }
        if let Some(v) = params.reward_cap_per_epoch {
            config.reward_cap_per_epoch = v;
        }
        if let Some(v) = params.paused {
            config.paused = v;
        }
        Ok(())
    }
}

// ----------------------------- Accounts -----------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,

    /// Must currently have `admin` as its mint authority.
    #[account(mut)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: only its pubkey is stored; this is the treasury authority/account.
    pub treasury: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintReward<'info> {
    pub minter: Signer<'info>,

    #[account(mut, seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, address = config.token_mint)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ CoreError::Unauthorized
    )]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct BurnSink<'info> {
    pub owner: Signer<'info>,

    #[account(seeds = [b"config"], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, address = config.token_mint)]
    pub token_mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub from: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

// ------------------------------ State -------------------------------

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub admin: Pubkey,             // upgrade/treasury authority
    pub authorized_minter: Pubkey, // backend signer allowed to mint rewards
    pub token_mint: Pubkey,        // native game currency
    pub treasury: Pubkey,          // collects marketplace fees / royalties
    pub marketplace_fee_bps: u16,  // protocol fee on sales
    pub royalty_bps: u16,          // creator royalty on secondary sales
    pub burn_share_bps: u16,       // % of fees burned (deflationary sink)
    pub reward_cap_per_epoch: u64, // hard emission ceiling per epoch
    pub minted_this_epoch: u64,    // running emission counter
    pub epoch: u64,
    pub paused: bool,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InitializeParams {
    pub authorized_minter: Pubkey,
    pub marketplace_fee_bps: u16,
    pub royalty_bps: u16,
    pub burn_share_bps: u16,
    pub reward_cap_per_epoch: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateConfigParams {
    pub authorized_minter: Option<Pubkey>,
    pub marketplace_fee_bps: Option<u16>,
    pub royalty_bps: Option<u16>,
    pub burn_share_bps: Option<u16>,
    pub reward_cap_per_epoch: Option<u64>,
    pub paused: Option<bool>,
}

// ------------------------------ Events ------------------------------

#[event]
pub struct Initialized {
    pub admin: Pubkey,
    pub token_mint: Pubkey,
}

#[event]
pub struct RewardMinted {
    pub recipient: Pubkey,
    pub amount: u64,
    pub epoch: u64,
}

#[event]
pub struct EpochAdvanced {
    pub epoch: u64,
}

#[event]
pub struct Burned {
    pub amount: u64,
}

// ------------------------------ Errors ------------------------------

#[error_code]
pub enum CoreError {
    #[msg("Caller is not authorized")]
    Unauthorized,
    #[msg("Basis points must be <= 10000")]
    InvalidBps,
    #[msg("Per-epoch reward cap exceeded")]
    EpochCapExceeded,
    #[msg("Minting is paused")]
    Paused,
    #[msg("Math overflow")]
    MathOverflow,
}
