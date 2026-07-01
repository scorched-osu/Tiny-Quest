// programs/arena/src/lib.rs
//
// Async PvP ladder. The backend runs the (deterministic, power-based) match sim
// off-chain, then calls submit_match here as the match_authority. On-chain we
// keep each player's Rating and update it: winner gains win_points + a streak
// bonus; loser loses loss_points down to a floor. Seasons reset lazily — a
// stale Rating is reset to base the next time it's touched.
//
// Cargo: anchor-lang = "0.31".

use anchor_lang::prelude::*;

declare_id!("23AMrskYWpeEQny9mwLsBW1j2e6XXdjY15tRaMXv5dHZ"); // placeholder

#[program]
pub mod game_arena {
    use super::*;

    pub fn init_arena(ctx: Context<InitArena>, params: ArenaParams) -> Result<()> {
        let a = &mut ctx.accounts.arena;
        a.admin = ctx.accounts.admin.key();
        a.match_authority = params.match_authority;
        a.base_rating = params.base_rating;
        a.win_points = params.win_points;
        a.loss_points = params.loss_points;
        a.streak_bonus = params.streak_bonus;
        a.rating_floor = params.rating_floor;
        a.season = 1;
        a.bump = ctx.bumps.arena;
        Ok(())
    }

    /// A player joins the current season's ladder.
    pub fn register(ctx: Context<Register>) -> Result<()> {
        let arena = &ctx.accounts.arena;
        let r = &mut ctx.accounts.rating;
        r.player = ctx.accounts.player.key();
        r.season = arena.season;
        r.rating = arena.base_rating;
        r.wins = 0;
        r.losses = 0;
        r.streak = 0;
        r.bump = ctx.bumps.rating;
        Ok(())
    }

    /// Backend submits a settled match. Updates both ratings.
    pub fn submit_match(ctx: Context<SubmitMatch>) -> Result<()> {
        let arena = &ctx.accounts.arena;
        require_keys_eq!(ctx.accounts.match_authority.key(), arena.match_authority, ArenaError::Unauthorized);

        season_sync(&mut ctx.accounts.winner_rating, arena);
        season_sync(&mut ctx.accounts.loser_rating, arena);

        let w = &mut ctx.accounts.winner_rating;
        let bonus = arena.streak_bonus.saturating_mul(w.streak);
        w.rating = w.rating.saturating_add(arena.win_points).saturating_add(bonus);
        w.wins = w.wins.saturating_add(1);
        w.streak = w.streak.saturating_add(1);

        let l = &mut ctx.accounts.loser_rating;
        l.rating = l.rating.saturating_sub(arena.loss_points).max(arena.rating_floor);
        l.losses = l.losses.saturating_add(1);
        l.streak = 0;

        emit!(MatchSettled {
            winner: ctx.accounts.winner_rating.player,
            loser: ctx.accounts.loser_rating.player,
            winner_rating: ctx.accounts.winner_rating.rating,
            loser_rating: ctx.accounts.loser_rating.rating,
            season: arena.season,
        });
        Ok(())
    }

    pub fn new_season(ctx: Context<AdminOnly>) -> Result<()> {
        let a = &mut ctx.accounts.arena;
        a.season = a.season.checked_add(1).ok_or(ArenaError::MathOverflow)?;
        emit!(NewSeason { season: a.season });
        Ok(())
    }

    pub fn set_match_authority(ctx: Context<AdminOnly>, new_authority: Pubkey) -> Result<()> {
        ctx.accounts.arena.match_authority = new_authority;
        Ok(())
    }
}

/// Reset a Rating to base if it belongs to a past season.
fn season_sync(r: &mut Account<Rating>, arena: &Account<Arena>) {
    if r.season != arena.season {
        r.season = arena.season;
        r.rating = arena.base_rating;
        r.wins = 0;
        r.losses = 0;
        r.streak = 0;
    }
}

// ----------------------------- accounts -----------------------------

#[derive(Accounts)]
pub struct InitArena<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + Arena::INIT_SPACE, seeds = [b"arena"], bump)]
    pub arena: Account<'info, Arena>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Register<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"arena"], bump = arena.bump)] pub arena: Account<'info, Arena>,
    #[account(init, payer = player, space = 8 + Rating::INIT_SPACE, seeds = [b"rating", player.key().as_ref()], bump)]
    pub rating: Account<'info, Rating>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitMatch<'info> {
    pub match_authority: Signer<'info>,
    #[account(seeds = [b"arena"], bump = arena.bump)] pub arena: Account<'info, Arena>,
    /// CHECK: winner player key (seed)
    pub winner: UncheckedAccount<'info>,
    /// CHECK: loser player key (seed)
    pub loser: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"rating", winner.key().as_ref()], bump = winner_rating.bump)]
    pub winner_rating: Account<'info, Rating>,
    #[account(mut, seeds = [b"rating", loser.key().as_ref()], bump = loser_rating.bump)]
    pub loser_rating: Account<'info, Rating>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"arena"], bump = arena.bump, has_one = admin @ ArenaError::Unauthorized)]
    pub arena: Account<'info, Arena>,
}

// ------------------------------ state -------------------------------

#[account]
#[derive(InitSpace)]
pub struct Arena {
    pub admin: Pubkey,
    pub match_authority: Pubkey,
    pub base_rating: u32,
    pub win_points: u32,
    pub loss_points: u32,
    pub streak_bonus: u32,
    pub rating_floor: u32,
    pub season: u16,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Rating {
    pub player: Pubkey,
    pub season: u16,
    pub rating: u32,
    pub wins: u32,
    pub losses: u32,
    pub streak: u32,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ArenaParams {
    pub match_authority: Pubkey,
    pub base_rating: u32,   // e.g. 1000
    pub win_points: u32,    // e.g. 25
    pub loss_points: u32,   // e.g. 20
    pub streak_bonus: u32,  // e.g. 3 per consecutive win
    pub rating_floor: u32,  // e.g. 100
}

#[event] pub struct MatchSettled { pub winner: Pubkey, pub loser: Pubkey, pub winner_rating: u32, pub loser_rating: u32, pub season: u16 }
#[event] pub struct NewSeason { pub season: u16 }

#[error_code]
pub enum ArenaError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Math overflow")] MathOverflow,
}
