// programs/guilds/src/lib.rs
//
// Guilds / Social. One guild per founder (PDA seeded by founder). Members hold a
// Membership PDA with a role and lifetime contribution. Contributions transfer
// Jade into the guild's vault ATA (owned by the guild PDA) to fund guild perks.
//
// Cargo: anchor-lang = "0.31", anchor-spl = "0.31".

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("GuiL1111111111111111111111111111111111111111"); // placeholder

pub const ROLE_MEMBER: u8 = 0;
pub const ROLE_OFFICER: u8 = 1;
pub const ROLE_LEADER: u8 = 2;

#[program]
pub mod game_guilds {
    use super::*;

    pub fn create_guild(ctx: Context<CreateGuild>, name: String, motto: String) -> Result<()> {
        require!(name.len() <= 32 && motto.len() <= 64, GuildError::TooLong);
        let g = &mut ctx.accounts.guild;
        g.founder = ctx.accounts.founder.key();
        g.name = name;
        g.motto = motto;
        g.payment_mint = ctx.accounts.payment_mint.key();
        g.member_count = 1;
        g.total_contribution = 0;
        g.bump = ctx.bumps.guild;

        let m = &mut ctx.accounts.membership;
        m.guild = g.key();
        m.player = ctx.accounts.founder.key();
        m.role = ROLE_LEADER;
        m.contribution = 0;
        m.bump = ctx.bumps.membership;
        Ok(())
    }

    pub fn join_guild(ctx: Context<JoinGuild>) -> Result<()> {
        let g = &mut ctx.accounts.guild;
        g.member_count = g.member_count.checked_add(1).ok_or(GuildError::MathOverflow)?;
        let m = &mut ctx.accounts.membership;
        m.guild = g.key();
        m.player = ctx.accounts.player.key();
        m.role = ROLE_MEMBER;
        m.contribution = 0;
        m.bump = ctx.bumps.membership;
        emit!(Joined { guild: g.key(), player: m.player });
        Ok(())
    }

    pub fn leave_guild(ctx: Context<LeaveGuild>) -> Result<()> {
        require!(ctx.accounts.membership.role != ROLE_LEADER, GuildError::LeaderMustTransfer);
        let g = &mut ctx.accounts.guild;
        g.member_count = g.member_count.saturating_sub(1);
        emit!(Left { guild: g.key(), player: ctx.accounts.player.key() });
        Ok(()) // membership closed via `close = player`
    }

    /// Contribute Jade into the guild vault; records lifetime contribution.
    pub fn contribute(ctx: Context<Contribute>, amount: u64) -> Result<()> {
        token_interface::transfer_checked(
            CpiContext::new(ctx.accounts.token_program.to_account_info(), TransferChecked {
                from: ctx.accounts.player_token_account.to_account_info(),
                mint: ctx.accounts.payment_mint.to_account_info(),
                to: ctx.accounts.guild_vault.to_account_info(),
                authority: ctx.accounts.player.to_account_info(),
            }),
            amount, ctx.accounts.payment_mint.decimals,
        )?;
        let g = &mut ctx.accounts.guild;
        g.total_contribution = g.total_contribution.checked_add(amount).ok_or(GuildError::MathOverflow)?;
        let m = &mut ctx.accounts.membership;
        m.contribution = m.contribution.checked_add(amount).ok_or(GuildError::MathOverflow)?;
        emit!(Contributed { guild: g.key(), player: m.player, amount, total: g.total_contribution });
        Ok(())
    }

    /// Leader/officer sets a member's role (cannot set another leader).
    pub fn set_role(ctx: Context<SetRole>, role: u8) -> Result<()> {
        require!(role <= ROLE_OFFICER, GuildError::InvalidRole);
        require!(ctx.accounts.actor_membership.role >= ROLE_OFFICER, GuildError::Unauthorized);
        ctx.accounts.target_membership.role = role;
        Ok(())
    }
}

// ----------------------------- accounts -----------------------------

#[derive(Accounts)]
#[instruction(name: String)]
pub struct CreateGuild<'info> {
    #[account(mut)] pub founder: Signer<'info>,
    #[account(init, payer = founder, space = 8 + Guild::INIT_SPACE, seeds = [b"guild", founder.key().as_ref()], bump)]
    pub guild: Account<'info, Guild>,
    #[account(init, payer = founder, space = 8 + Membership::INIT_SPACE, seeds = [b"member", guild.key().as_ref(), founder.key().as_ref()], bump)]
    pub membership: Account<'info, Membership>,
    pub payment_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinGuild<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(mut)] pub guild: Account<'info, Guild>,
    #[account(init, payer = player, space = 8 + Membership::INIT_SPACE, seeds = [b"member", guild.key().as_ref(), player.key().as_ref()], bump)]
    pub membership: Account<'info, Membership>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct LeaveGuild<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(mut)] pub guild: Account<'info, Guild>,
    #[account(mut, close = player, seeds = [b"member", guild.key().as_ref(), player.key().as_ref()], bump = membership.bump,
        has_one = player @ GuildError::Unauthorized)]
    pub membership: Account<'info, Membership>,
}

#[derive(Accounts)]
pub struct Contribute<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(mut)] pub guild: Account<'info, Guild>,
    #[account(mut, seeds = [b"member", guild.key().as_ref(), player.key().as_ref()], bump = membership.bump,
        has_one = player @ GuildError::Unauthorized)]
    pub membership: Account<'info, Membership>,
    #[account(address = guild.payment_mint)] pub payment_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)] pub player_token_account: InterfaceAccount<'info, TokenAccount>,
    /// vault ATA owned by the guild PDA
    #[account(mut, constraint = guild_vault.owner == guild.key() @ GuildError::BadVault)]
    pub guild_vault: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct SetRole<'info> {
    pub actor: Signer<'info>,
    pub guild: Account<'info, Guild>,
    #[account(seeds = [b"member", guild.key().as_ref(), actor.key().as_ref()], bump = actor_membership.bump,
        constraint = actor_membership.player == actor.key() @ GuildError::Unauthorized)]
    pub actor_membership: Account<'info, Membership>,
    /// CHECK: target player key (seed)
    pub target: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"member", guild.key().as_ref(), target.key().as_ref()], bump = target_membership.bump)]
    pub target_membership: Account<'info, Membership>,
}

// ------------------------------ state -------------------------------

#[account]
#[derive(InitSpace)]
pub struct Guild {
    pub founder: Pubkey,
    #[max_len(32)] pub name: String,
    #[max_len(64)] pub motto: String,
    pub payment_mint: Pubkey,
    pub member_count: u32,
    pub total_contribution: u64,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Membership {
    pub guild: Pubkey,
    pub player: Pubkey,
    pub role: u8,
    pub contribution: u64,
    pub bump: u8,
}

#[event] pub struct Joined { pub guild: Pubkey, pub player: Pubkey }
#[event] pub struct Left { pub guild: Pubkey, pub player: Pubkey }
#[event] pub struct Contributed { pub guild: Pubkey, pub player: Pubkey, pub amount: u64, pub total: u64 }

#[error_code]
pub enum GuildError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Name or motto too long")] TooLong,
    #[msg("Invalid role")] InvalidRole,
    #[msg("Leader must transfer leadership before leaving")] LeaderMustTransfer,
    #[msg("Vault not owned by guild")] BadVault,
    #[msg("Math overflow")] MathOverflow,
}
