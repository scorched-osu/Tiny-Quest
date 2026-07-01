// programs/skills/src/lib.rs
//
// Per-hero skill tree. Fully trustless and backend-free: a hero's total skill
// points are DERIVED from its on-chain level (points_per_level * level), and the
// book records only what's been spent. available = level*PPL - spent. You can
// never get more points than your level grants, and the chain is the source of
// truth for both level (read from the hero's Attributes) and spend.
//
// Skill EFFECTS (damage %, buffs, cooldowns) live in the off-chain combat sim,
// keyed by (skill_id, level). This program owns ownership + economy only.
//
// Cargo: anchor-lang = "0.31", mpl-core = "0.8".

use anchor_lang::prelude::*;
use mpl_core::{accounts::BaseAssetV1, fetch_plugin, types::{Attributes, PluginType}};

declare_id!("SkiL1111111111111111111111111111111111111111"); // placeholder

const POINTS_PER_LEVEL: u32 = 1;
const LEARN_COST: u32 = 1;
const MAX_SKILL_LEVEL: u8 = 10;
const MAX_SKILLS: usize = 32;

#[program]
pub mod game_skills {
    use super::*;

    /// Owner opens a skill book for their hero (one per hero asset).
    pub fn init_book(ctx: Context<InitBook>) -> Result<()> {
        require_keys_eq!(read_owner(&ctx.accounts.asset)?, ctx.accounts.owner.key(), SkillError::NotOwner);
        let b = &mut ctx.accounts.book;
        b.hero = ctx.accounts.asset.key();
        b.spent = 0;
        b.skills = Vec::new();
        b.bump = ctx.bumps.book;
        Ok(())
    }

    /// Learn a new skill at level 1.
    pub fn learn_skill(ctx: Context<EditBook>, skill_id: u16) -> Result<()> {
        require_keys_eq!(read_owner(&ctx.accounts.asset)?, ctx.accounts.owner.key(), SkillError::NotOwner);
        let level = read_level(&ctx.accounts.asset)?;
        let b = &mut ctx.accounts.book;

        require!(b.skills.iter().all(|s| s.id != skill_id), SkillError::AlreadyLearned);
        require!(b.skills.len() < MAX_SKILLS, SkillError::BookFull);
        spend(b, level, LEARN_COST)?;

        b.skills.push(Skill { id: skill_id, level: 1 });
        emit!(SkillLearned { hero: b.hero, skill_id });
        Ok(())
    }

    /// Rank up a learned skill. Cost = current level (escalating).
    pub fn upgrade_skill(ctx: Context<EditBook>, skill_id: u16) -> Result<()> {
        require_keys_eq!(read_owner(&ctx.accounts.asset)?, ctx.accounts.owner.key(), SkillError::NotOwner);
        let level = read_level(&ctx.accounts.asset)?;
        let b = &mut ctx.accounts.book;

        let idx = b.skills.iter().position(|s| s.id == skill_id).ok_or(SkillError::NotLearned)?;
        let cur = b.skills[idx].level;
        require!(cur < MAX_SKILL_LEVEL, SkillError::MaxLevel);
        spend(b, level, cur as u32)?; // cost scales with current rank

        b.skills[idx].level = cur + 1;
        emit!(SkillUpgraded { hero: b.hero, skill_id, level: cur + 1 });
        Ok(())
    }
}

// ----------------------------- helpers -----------------------------

fn available(spent: u32, hero_level: u16) -> u32 {
    (hero_level as u32).saturating_mul(POINTS_PER_LEVEL).saturating_sub(spent)
}
fn spend(b: &mut SkillBook, hero_level: u16, cost: u32) -> Result<()> {
    require!(available(b.spent, hero_level) >= cost, SkillError::NotEnoughPoints);
    b.spent = b.spent.checked_add(cost).ok_or(SkillError::MathOverflow)?;
    Ok(())
}
fn read_owner(asset: &UncheckedAccount) -> Result<Pubkey> {
    let data = asset.try_borrow_data()?;
    Ok(BaseAssetV1::from_bytes(&data).map_err(|_| SkillError::BadAsset)?.owner)
}
fn read_level(asset: &UncheckedAccount) -> Result<u16> {
    let (_, attrs, _) = fetch_plugin::<BaseAssetV1, Attributes>(&asset.to_account_info(), PluginType::Attributes)
        .map_err(|_| SkillError::BadAsset)?;
    Ok(attrs.attribute_list.iter().find(|a| a.key == "level")
        .and_then(|a| a.value.parse::<u16>().ok()).unwrap_or(1))
}

// ----------------------------- accounts ----------------------------

#[derive(Accounts)]
pub struct InitBook<'info> {
    #[account(mut)] pub owner: Signer<'info>,
    /// CHECK: hero asset (ownership + level read from it)
    pub asset: UncheckedAccount<'info>,
    #[account(init, payer = owner, space = 8 + SkillBook::INIT_SPACE, seeds = [b"skills", asset.key().as_ref()], bump)]
    pub book: Account<'info, SkillBook>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EditBook<'info> {
    pub owner: Signer<'info>,
    /// CHECK: hero asset (ownership + level read from it; seed binds the book to it)
    pub asset: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"skills", asset.key().as_ref()], bump = book.bump)]
    pub book: Account<'info, SkillBook>,
}

// ------------------------------ state ------------------------------

#[account]
#[derive(InitSpace)]
pub struct SkillBook {
    pub hero: Pubkey,
    pub spent: u32,
    #[max_len(MAX_SKILLS)]
    pub skills: Vec<Skill>,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct Skill {
    pub id: u16,
    pub level: u8,
}

#[event] pub struct SkillLearned { pub hero: Pubkey, pub skill_id: u16 }
#[event] pub struct SkillUpgraded { pub hero: Pubkey, pub skill_id: u16, pub level: u8 }

#[error_code]
pub enum SkillError {
    #[msg("Caller is not the hero owner")] NotOwner,
    #[msg("Could not read asset")] BadAsset,
    #[msg("Skill already learned")] AlreadyLearned,
    #[msg("Skill not learned")] NotLearned,
    #[msg("Skill at max level")] MaxLevel,
    #[msg("Skill book is full")] BookFull,
    #[msg("Not enough skill points")] NotEnoughPoints,
    #[msg("Math overflow")] MathOverflow,
}
