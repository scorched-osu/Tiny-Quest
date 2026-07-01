// programs/titles/src/lib.rs
//
// Titles / achievements. The backend (grant_authority) awards title IDs when a
// player hits a milestone; the player equips one for display/perks. Off-chain
// maps title_id -> name, art, and any stat perk applied in the combat sim.
//
// Cargo: anchor-lang = "0.31".

use anchor_lang::prelude::*;

declare_id!("23AMrskYWpeEQny9mwLsBW1jZQyqFyy8oLu6uY5cc76X"); // placeholder

const MAX_TITLES: usize = 128;

#[program]
pub mod game_titles {
    use super::*;

    pub fn init_config(ctx: Context<InitConfig>, grant_authority: Pubkey) -> Result<()> {
        let c = &mut ctx.accounts.config;
        c.admin = ctx.accounts.admin.key();
        c.grant_authority = grant_authority;
        c.bump = ctx.bumps.config;
        Ok(())
    }

    /// Player opens their title shelf (once).
    pub fn init_shelf(ctx: Context<InitShelf>) -> Result<()> {
        let s = &mut ctx.accounts.shelf;
        s.player = ctx.accounts.player.key();
        s.owned = Vec::new();
        s.equipped = u16::MAX; // none
        s.bump = ctx.bumps.shelf;
        Ok(())
    }

    /// Backend grants a title to a player.
    pub fn grant_title(ctx: Context<GrantTitle>, title_id: u16) -> Result<()> {
        require_keys_eq!(ctx.accounts.grant_authority.key(), ctx.accounts.config.grant_authority, TitleError::Unauthorized);
        let s = &mut ctx.accounts.shelf;
        require!(!s.owned.contains(&title_id), TitleError::AlreadyOwned);
        require!(s.owned.len() < MAX_TITLES, TitleError::ShelfFull);
        s.owned.push(title_id);
        emit!(TitleGranted { player: s.player, title_id });
        Ok(())
    }

    /// Player equips a title they own (or u16::MAX to clear).
    pub fn equip_title(ctx: Context<EquipTitle>, title_id: u16) -> Result<()> {
        let s = &mut ctx.accounts.shelf;
        require!(title_id == u16::MAX || s.owned.contains(&title_id), TitleError::NotOwned);
        s.equipped = title_id;
        emit!(TitleEquipped { player: s.player, title_id });
        Ok(())
    }
}

// ----------------------------- accounts -----------------------------

#[derive(Accounts)]
pub struct InitConfig<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + TitleConfig::INIT_SPACE, seeds = [b"title_config"], bump)]
    pub config: Account<'info, TitleConfig>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitShelf<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(init, payer = player, space = 8 + TitleShelf::INIT_SPACE, seeds = [b"titles", player.key().as_ref()], bump)]
    pub shelf: Account<'info, TitleShelf>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GrantTitle<'info> {
    pub grant_authority: Signer<'info>,
    #[account(seeds = [b"title_config"], bump = config.bump)] pub config: Account<'info, TitleConfig>,
    /// CHECK: title owner (seed)
    pub player: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"titles", player.key().as_ref()], bump = shelf.bump)]
    pub shelf: Account<'info, TitleShelf>,
}

#[derive(Accounts)]
pub struct EquipTitle<'info> {
    pub player: Signer<'info>,
    #[account(mut, seeds = [b"titles", player.key().as_ref()], bump = shelf.bump, has_one = player @ TitleError::Unauthorized)]
    pub shelf: Account<'info, TitleShelf>,
}

// ------------------------------ state -------------------------------

#[account]
#[derive(InitSpace)]
pub struct TitleConfig {
    pub admin: Pubkey,
    pub grant_authority: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct TitleShelf {
    pub player: Pubkey,
    #[max_len(MAX_TITLES)] pub owned: Vec<u16>,
    pub equipped: u16,
    pub bump: u8,
}

#[event] pub struct TitleGranted { pub player: Pubkey, pub title_id: u16 }
#[event] pub struct TitleEquipped { pub player: Pubkey, pub title_id: u16 }

#[error_code]
pub enum TitleError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Title already owned")] AlreadyOwned,
    #[msg("Title not owned")] NotOwned,
    #[msg("Shelf is full")] ShelfFull,
}
