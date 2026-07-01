// programs/progression/src/lib.rs  (v2 — gear enhancement + hero XP leveling)
//
// This single program owns ALL stat mutations because it holds the
// [b"upgrade_auth"] PDA registered as assets_config.upgrade_authority, which is
// the only key game_assets::set_asset_attributes accepts. Two subsystems:
//
//   GEAR ENHANCEMENT (+N):
//     upgrade_guaranteed / attempt_chance / resolve_chance
//     Quality/slot-aware caps, escalating token sink, commit-reveal rolls.
//
//   HERO XP LEVELING:
//     grant_xp     — backend-authoritative XP, runs the level-up loop
//     allocate_stat— owner spends earned points into a stat
//
// Anti-cheat throughout: current values are read from the asset's on-chain
// Attributes plugin (fetch_plugin), never trusted from the client.
//
// Cross-program wiring (once, at deploy):
//   Cargo: game_core = { path="../core", features=["cpi"] }
//          game_assets = { path="../assets", features=["cpi"] }
//   Set assets_config.upgrade_authority = this program's [b"upgrade_auth"] PDA.
//   game_assets must expose set_asset_attributes (accepts heroes + gear collections).

use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;
use mpl_core::{accounts::BaseAssetV1, fetch_plugin, types::{Attribute, Attributes, PluginType}};

use game_core::cpi as core_cpi;
use game_core::cpi::accounts::BurnSink as CoreBurnSink;
use game_assets::cpi as assets_cpi;
use game_assets::cpi::accounts::SetAssetAttributes;
use game_assets::AttrArg;
use game_resources::cpi as resources_cpi;
use game_resources::cpi::accounts::BurnResource as ResourceBurn;

declare_id!("PrGr1111111111111111111111111111111111111111"); // placeholder

const QUALITY_EPIC: u8 = 3;
const MAX_STARS: u8 = 10;
const MAX_LEVELUPS_PER_CALL: u16 = 50;

#[program]
pub mod game_progression {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, params: ProgConfigParams) -> Result<()> {
        let c = &mut ctx.accounts.config;
        c.admin = ctx.accounts.admin.key();
        c.roll_authority = params.roll_authority;
        c.xp_authority = params.xp_authority;
        c.core_program = params.core_program;
        c.assets_program = params.assets_program;
        c.token_mint = params.token_mint;
        c.base_token_cost = params.base_token_cost;
        c.base_success_bps = params.base_success_bps;
        c.success_decay_bps = params.success_decay_bps;
        c.min_success_bps = params.min_success_bps;
        c.committed_seed_hash = params.committed_seed_hash;
        c.base_xp = params.base_xp;
        c.xp_per_level = params.xp_per_level;
        c.xp_quad = params.xp_quad;
        c.max_level = params.max_level;
        c.growth_str = params.growth_str;
        c.growth_dex = params.growth_dex;
        c.growth_int = params.growth_int;
        c.growth_vit = params.growth_vit;
        c.points_per_level = params.points_per_level;
        c.soul_mint = params.soul_mint;
        c.resources_program = params.resources_program;
        c.base_star_cost = params.base_star_cost;
        c.star_cost_growth = params.star_cost_growth;
        c.max_stars = params.max_stars;
        c.config_bump = ctx.bumps.config;
        c.upgrade_auth_bump = ctx.bumps.upgrade_authority;
        Ok(())
    }

    // =================== HERO XP LEVELING ===================

    /// Backend-authoritative XP grant. Adds XP and processes any level-ups,
    /// applying auto stat growth and awarding allocatable points per level.
    pub fn grant_xp(ctx: Context<HeroMutate>, amount: u64) -> Result<()> {
        require_keys_eq!(ctx.accounts.authority.key(), ctx.accounts.config.xp_authority, ProgError::Unauthorized);

        let cfg = &ctx.accounts.config;
        let mut list = read_attribute_list(&ctx.accounts.asset)?;

        let mut level = get_u64(&list, "level").max(1) as u16;
        let mut xp = get_u64(&list, "exp");
        let mut str_ = get_u64(&list, "strength") as u32;
        let mut dex = get_u64(&list, "agility") as u32;
        let mut int_ = get_u64(&list, "intelligence") as u32;
        let mut vit = get_u64(&list, "vitality") as u32;
        let mut unspent = get_u64(&list, "unspent") as u32;
        let (gs, ga, gi, gv) = class_growth(get_u64(&list, "class") as u8);

        xp = xp.checked_add(amount).ok_or(ProgError::MathOverflow)?;

        let mut gained: u16 = 0;
        while level < cfg.max_level && gained < MAX_LEVELUPS_PER_CALL {
            let req = xp_for_level(cfg, level);
            if xp < req { break; }
            xp -= req;
            level += 1;
            str_ += gs;
            dex += ga;
            int_ += gi;
            vit += gv;
            unspent += cfg.points_per_level as u32;
            gained += 1;
        }

        upsert(&mut list, "level", level.to_string());
        upsert(&mut list, "exp", xp.to_string());
        upsert(&mut list, "strength", str_.to_string());
        upsert(&mut list, "agility", dex.to_string());
        upsert(&mut list, "intelligence", int_.to_string());
        upsert(&mut list, "vitality", vit.to_string());
        upsert(&mut list, "unspent", unspent.to_string());

        write_attrs(&ctx, list)?;
        emit!(XpGranted { asset: ctx.accounts.asset.key(), amount, level, levels_gained: gained });
        Ok(())
    }

    /// Owner spends earned points into one stat. Ownership + balance are read
    /// from chain; the upgrade-auth PDA performs the protected write.
    pub fn allocate_stat(ctx: Context<HeroMutate>, stat: StatKind, amount: u32) -> Result<()> {
        require!(amount > 0, ProgError::ZeroAmount);
        let owner = read_asset_owner(&ctx.accounts.asset)?;
        require_keys_eq!(owner, ctx.accounts.authority.key(), ProgError::NotOwner);

        let mut list = read_attribute_list(&ctx.accounts.asset)?;
        let unspent = get_u64(&list, "unspent") as u32;
        require!(unspent >= amount, ProgError::InsufficientPoints);

        let key = stat.key();
        let current = get_u64(&list, key) as u32;
        upsert(&mut list, key, (current + amount).to_string());
        upsert(&mut list, "unspent", (unspent - amount).to_string());

        write_attrs(&ctx, list)?;
        emit!(StatAllocated { asset: ctx.accounts.asset.key(), stat: stat as u8, amount });
        Ok(())
    }

    // =================== GEAR ENHANCEMENT ===================

    pub fn upgrade_guaranteed(ctx: Context<UpgradeGuaranteed>, class: UpgradeClass, quality: u8) -> Result<()> {
        let current = read_plus(&ctx.accounts.asset)?;
        let target = current.checked_add(1).ok_or(ProgError::MathOverflow)?;
        require!(target <= guaranteed_cap(class, quality), ProgError::NotGuaranteed);

        let cost = upgrade_cost(ctx.accounts.config.base_token_cost, target)?;
        charge(&ctx.accounts.core_program, &ctx.accounts.core_config, &ctx.accounts.token_mint,
               &ctx.accounts.player_token_account, &ctx.accounts.player, &ctx.accounts.token_program, cost)?;

        let new_list = compute_gear_attrs(&ctx.accounts.asset, class, target)?;
        apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
                    &ctx.accounts.player, &ctx.accounts.asset, &ctx.accounts.collection,
                    &ctx.accounts.assets_update_authority, &ctx.accounts.mpl_core_program,
                    &ctx.accounts.system_program, ctx.accounts.config.upgrade_auth_bump, new_list)?;

        emit!(Upgraded { asset: ctx.accounts.asset.key(), level: target, success: true, guaranteed: true });
        Ok(())
    }

    pub fn attempt_chance(ctx: Context<AttemptChance>, class: UpgradeClass, quality: u8, client_seed: [u8; 32]) -> Result<()> {
        let current = read_plus(&ctx.accounts.asset)?;
        let target = current.checked_add(1).ok_or(ProgError::MathOverflow)?;
        let cap = guaranteed_cap(class, quality);
        require!(target > cap, ProgError::UseGuaranteed);

        let cost = upgrade_cost(ctx.accounts.config.base_token_cost, target)?;
        charge(&ctx.accounts.core_program, &ctx.accounts.core_config, &ctx.accounts.token_mint,
               &ctx.accounts.player_token_account, &ctx.accounts.player, &ctx.accounts.token_program, cost)?;

        let over = target - cap;
        let success_bps = success_rate(&ctx.accounts.config, over);

        let p = &mut ctx.accounts.pending;
        p.owner = ctx.accounts.player.key();
        p.asset = ctx.accounts.asset.key();
        p.class = class as u8;
        p.quality = quality;
        p.target_level = target;
        p.success_bps = success_bps;
        p.client_seed = client_seed;
        p.committed_seed_hash = ctx.accounts.config.committed_seed_hash;
        p.bump = ctx.bumps.pending;

        emit!(UpgradePending { asset: p.asset, target, success_bps });
        Ok(())
    }

    pub fn resolve_chance(ctx: Context<ResolveChance>, revealed_seed: [u8; 32], next_seed_hash: [u8; 32]) -> Result<()> {
        require_keys_eq!(ctx.accounts.roll_authority.key(), ctx.accounts.config.roll_authority, ProgError::Unauthorized);

        let p = &ctx.accounts.pending;
        require!(keccak::hash(&revealed_seed).0 == p.committed_seed_hash, ProgError::BadReveal);

        let mut buf = Vec::with_capacity(98);
        buf.extend_from_slice(&revealed_seed);
        buf.extend_from_slice(&p.client_seed);
        buf.extend_from_slice(p.asset.as_ref());
        buf.extend_from_slice(&p.target_level.to_le_bytes());
        let rand = keccak::hash(&buf).0;
        let roll = u16::from_le_bytes([rand[0], rand[1]]) % 10_000;
        let success = roll < p.success_bps;

        if success {
            let class = UpgradeClass::from_u8(p.class)?;
            let new_list = compute_gear_attrs(&ctx.accounts.asset, class, p.target_level)?;
            apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
                        &ctx.accounts.roll_authority, &ctx.accounts.asset, &ctx.accounts.collection,
                        &ctx.accounts.assets_update_authority, &ctx.accounts.mpl_core_program,
                        &ctx.accounts.system_program, ctx.accounts.config.upgrade_auth_bump, new_list)?;
        }

        ctx.accounts.config.committed_seed_hash = next_seed_hash;
        emit!(UpgradeResolved { asset: p.asset, success, roll, target: p.target_level });
        Ok(())
    }

    /// AWAKENING (star track). Guaranteed +1 star up to max_stars, paid in Soul
    /// (burned via the resource controller). Stars gate higher affix tiers,
    /// applied off-chain at battle/display time from the item's base affixes.
    pub fn awaken_item(ctx: Context<AwakenItem>) -> Result<()> {
        let cfg = &ctx.accounts.config;
        let mut list = read_attribute_list(&ctx.accounts.asset)?;
        let stars = get_u64(&list, "stars").max(1) as u8;
        require!(stars < cfg.max_stars, ProgError::MaxStars);
        let target = stars + 1;

        let cost = star_cost(cfg, target);
        resources_cpi::burn_resource(
            CpiContext::new(ctx.accounts.resources_program.to_account_info(), ResourceBurn {
                owner: ctx.accounts.player.to_account_info(),
                controller: ctx.accounts.soul_controller.to_account_info(),
                mint: ctx.accounts.soul_mint.to_account_info(),
                from: ctx.accounts.player_soul_account.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            }),
            cost,
        )?;

        upsert(&mut list, "stars", target.to_string());
        let attrs: Vec<AttrArg> = list.into_iter().map(|a| AttrArg { key: a.key, value: a.value }).collect();
        apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
            &ctx.accounts.player, &ctx.accounts.asset, &ctx.accounts.collection,
            &ctx.accounts.assets_update_authority, &ctx.accounts.mpl_core_program,
            &ctx.accounts.system_program, cfg.upgrade_auth_bump, attrs)?;

        emit!(Awakened { asset: ctx.accounts.asset.key(), stars: target, soul_burned: cost });
        Ok(())
    }

    /// GEM SOCKETING. Burns one gem token (a fungible resource mint) and records
    /// its mint in the gear's socket_<i> slot. Socket count derives from quality.
    /// The gem's stat bonus is applied off-chain from the recorded mint, keeping
    /// on-chain data lean (same philosophy as affixes/stars/plus).
    pub fn socket_gem(ctx: Context<SocketGem>, socket_index: u8) -> Result<()> {
        require_keys_eq!(read_asset_owner(&ctx.accounts.asset)?, ctx.accounts.player.key(), ProgError::NotOwner);
        let mut list = read_attribute_list(&ctx.accounts.asset)?;
        let quality = get_u64(&list, "quality") as u8;
        require!(socket_index < socket_slots(quality), ProgError::SocketOutOfRange);
        let key = format!("socket_{}", socket_index);
        require!(!list.iter().any(|a| a.key == key && !a.value.is_empty()), ProgError::SocketOccupied);

        resources_cpi::burn_resource(
            CpiContext::new(ctx.accounts.resources_program.to_account_info(), ResourceBurn {
                owner: ctx.accounts.player.to_account_info(),
                controller: ctx.accounts.gem_controller.to_account_info(),
                mint: ctx.accounts.gem_mint.to_account_info(),
                from: ctx.accounts.player_gem_account.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            }),
            1, // gems are 0-decimal: one token == one gem
        )?;

        upsert(&mut list, &key, ctx.accounts.gem_mint.key().to_string());
        let attrs: Vec<AttrArg> = list.into_iter().map(|a| AttrArg { key: a.key, value: a.value }).collect();
        apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
            &ctx.accounts.player, &ctx.accounts.asset, &ctx.accounts.collection,
            &ctx.accounts.assets_update_authority, &ctx.accounts.mpl_core_program,
            &ctx.accounts.system_program, ctx.accounts.config.upgrade_auth_bump, attrs)?;

        emit!(GemSocketed { asset: ctx.accounts.asset.key(), socket_index, gem: ctx.accounts.gem_mint.key() });
        Ok(())
    }

    /// Frees a socket. The gem is consumed on removal (not refunded), so this
    /// only makes sense to re-slot a better gem. Off-chain reads empty as free.
    pub fn unsocket_gem(ctx: Context<GearWrite>, socket_index: u8) -> Result<()> {
        require_keys_eq!(read_asset_owner(&ctx.accounts.asset)?, ctx.accounts.player.key(), ProgError::NotOwner);
        let mut list = read_attribute_list(&ctx.accounts.asset)?;
        let key = format!("socket_{}", socket_index);
        require!(list.iter().any(|a| a.key == key && !a.value.is_empty()), ProgError::SocketEmpty);
        upsert(&mut list, &key, String::new());
        let attrs: Vec<AttrArg> = list.into_iter().map(|a| AttrArg { key: a.key, value: a.value }).collect();
        apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
            &ctx.accounts.player, &ctx.accounts.asset, &ctx.accounts.collection,
            &ctx.accounts.assets_update_authority, &ctx.accounts.mpl_core_program,
            &ctx.accounts.system_program, ctx.accounts.config.upgrade_auth_bump, attrs)?;
        emit!(GemRemoved { asset: ctx.accounts.asset.key(), socket_index });
        Ok(())
    }

    pub fn set_roll_authority(ctx: Context<AdminOnly>, new_authority: Pubkey) -> Result<()> {
        ctx.accounts.config.roll_authority = new_authority; Ok(())
    }
    pub fn set_xp_authority(ctx: Context<AdminOnly>, new_authority: Pubkey) -> Result<()> {
        ctx.accounts.config.xp_authority = new_authority; Ok(())
    }
}

// ----------------------------- rules -----------------------------

fn guaranteed_cap(class: UpgradeClass, quality: u8) -> u16 {
    if quality >= QUALITY_EPIC { return 0; }
    match class { UpgradeClass::Armor => 4, UpgradeClass::Weapon => 6, UpgradeClass::Accessory => 0 }
}
fn success_rate(cfg: &ProgressionConfig, over: u16) -> u16 {
    let decay = cfg.success_decay_bps.saturating_mul(over.saturating_sub(1));
    cfg.base_success_bps.saturating_sub(decay).max(cfg.min_success_bps)
}
fn upgrade_cost(base: u64, target: u16) -> Result<u64> {
    base.checked_mul(target as u64).ok_or(ProgError::MathOverflow.into())
}
/// Soul cost to reach `target` stars: base + growth*(target-1), escalating.
fn star_cost(cfg: &ProgressionConfig, target: u8) -> u64 {
    cfg.base_star_cost + cfg.star_cost_growth * (target.saturating_sub(1) as u64)
}
/// Gem sockets available on a piece of gear, by quality tier (0 Common .. 6 Mythic).
fn socket_slots(quality: u8) -> u8 {
    match quality {
        0 => 0,      // Common
        1 | 2 => 1,  // Uncommon, Rare
        3 | 4 => 2,  // Epic, Legendary
        _ => 3,      // Unique, Mythic+
    }
}
/// Per-class growth per level: (strength, agility, intelligence, vitality).
/// Class ids match game_assets::HeroClass (0 Warrior .. 5 Wizard). Tune freely.
fn class_growth(class: u8) -> (u32, u32, u32, u32) {
    match class {
        0 => (2, 1, 0, 2), // Warrior   — STR/VIT bruiser
        1 => (0, 1, 3, 1), // Pyromancer — INT burst
        2 => (1, 3, 0, 1), // Archer    — AGI ranged
        3 => (2, 2, 0, 1), // Rogue     — STR/AGI
        4 => (1, 0, 0, 4), // Guardian  — VIT tank
        5 => (0, 1, 3, 1), // Wizard    — INT arcane
        _ => (1, 1, 1, 1),
    }
}
/// XP required to go from `level` -> `level+1`.
fn xp_for_level(cfg: &ProgressionConfig, level: u16) -> u64 {
    let l = (level.saturating_sub(1)) as u64;
    cfg.base_xp as u64 + cfg.xp_per_level as u64 * l + cfg.xp_quad as u64 * l * l
}

// --------------------- attribute read / merge --------------------

fn read_attribute_list(asset: &UncheckedAccount) -> Result<Vec<Attribute>> {
    let (_, attributes, _) = fetch_plugin::<BaseAssetV1, Attributes>(&asset.to_account_info(), PluginType::Attributes)
        .map_err(|_| ProgError::MissingAttributes)?;
    Ok(attributes.attribute_list)
}
fn read_plus(asset: &UncheckedAccount) -> Result<u16> {
    Ok(read_attribute_list(asset)?.iter().find(|a| a.key == "plus")
        .and_then(|a| a.value.parse::<u16>().ok()).unwrap_or(0))
}
fn read_asset_owner(asset: &UncheckedAccount) -> Result<Pubkey> {
    let data = asset.try_borrow_data()?;
    let base = BaseAssetV1::from_bytes(&data).map_err(|_| ProgError::BadAsset)?;
    Ok(base.owner)
}
fn get_u64(list: &[Attribute], key: &str) -> u64 {
    list.iter().find(|a| a.key == key).and_then(|a| a.value.parse::<u64>().ok()).unwrap_or(0)
}
fn upsert(list: &mut Vec<Attribute>, key: &str, value: String) {
    if let Some(a) = list.iter_mut().find(|a| a.key == key) { a.value = value; }
    else { list.push(Attribute { key: key.to_string(), value }); }
}
fn compute_gear_attrs(asset: &UncheckedAccount, _class: UpgradeClass, target: u16) -> Result<Vec<AttrArg>> {
    // Enhancement only records the new +level. The per-affix bonus shown as
    // `total(base+bonus)` in the UI is derived off-chain from the item's base
    // affixes plus its plus/stars — so we never store synthetic bonus keys.
    let mut list = read_attribute_list(asset)?;
    upsert(&mut list, "plus", target.to_string());
    Ok(list.into_iter().map(|a| AttrArg { key: a.key, value: a.value }).collect())
}

// --------------------------- CPI helpers -------------------------

#[allow(clippy::too_many_arguments)]
fn charge<'info>(core_program: &AccountInfo<'info>, core_config: &AccountInfo<'info>, token_mint: &AccountInfo<'info>,
    player_token_account: &AccountInfo<'info>, player: &Signer<'info>, token_program: &AccountInfo<'info>, amount: u64) -> Result<()> {
    core_cpi::burn_sink(CpiContext::new(core_program.clone(), CoreBurnSink {
        owner: player.to_account_info(), config: core_config.clone(), token_mint: token_mint.clone(),
        from: player_token_account.clone(), token_program: token_program.clone(),
    }), amount)
}

#[allow(clippy::too_many_arguments)]
fn apply_attrs<'info>(assets_program: &AccountInfo<'info>, assets_config: &AccountInfo<'info>, upgrade_authority: &AccountInfo<'info>,
    payer: &Signer<'info>, asset: &UncheckedAccount<'info>, collection: &AccountInfo<'info>, assets_update_authority: &AccountInfo<'info>,
    mpl_core_program: &AccountInfo<'info>, system_program: &AccountInfo<'info>, upgrade_auth_bump: u8, attrs: Vec<AttrArg>) -> Result<()> {
    let seeds: &[&[u8]] = &[b"upgrade_auth", &[upgrade_auth_bump]];
    assets_cpi::set_asset_attributes(CpiContext::new_with_signer(assets_program.clone(), SetAssetAttributes {
        assets_config: assets_config.clone(), upgrade_authority: upgrade_authority.clone(), payer: payer.to_account_info(),
        asset: asset.to_account_info(), collection: collection.clone(), update_authority: assets_update_authority.clone(),
        mpl_core_program: mpl_core_program.clone(), system_program: system_program.clone(),
    }, &[seeds]), attrs)
}

/// Convenience wrapper used by the hero instructions (collection = heroes).
fn write_attrs(ctx: &Context<HeroMutate>, list: Vec<Attribute>) -> Result<()> {
    let attrs: Vec<AttrArg> = list.into_iter().map(|a| AttrArg { key: a.key, value: a.value }).collect();
    apply_attrs(&ctx.accounts.assets_program, &ctx.accounts.assets_config, &ctx.accounts.upgrade_authority,
        &ctx.accounts.authority, &ctx.accounts.asset, &ctx.accounts.collection, &ctx.accounts.assets_update_authority,
        &ctx.accounts.mpl_core_program, &ctx.accounts.system_program, ctx.accounts.config.upgrade_auth_bump, attrs)
}

// ---------------------------- accounts ---------------------------

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + ProgressionConfig::INIT_SPACE, seeds = [b"prog_config"], bump)]
    pub config: Account<'info, ProgressionConfig>,
    /// CHECK: PDA registered as assets_config.upgrade_authority
    #[account(seeds = [b"upgrade_auth"], bump)] pub upgrade_authority: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Shared context for hero stat writes (grant_xp, allocate_stat).
#[derive(Accounts)]
pub struct HeroMutate<'info> {
    #[account(mut)] pub authority: Signer<'info>, // xp_authority (grant) OR hero owner (allocate)
    #[account(seeds = [b"prog_config"], bump = config.config_bump)]
    pub config: Account<'info, ProgressionConfig>,
    /// CHECK: hero asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: heroes collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: our upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets collection update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpgradeGuaranteed<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    /// CHECK: gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: game_core program
    #[account(address = config.core_program)] pub core_program: UncheckedAccount<'info>,
    /// CHECK: core config PDA
    #[account(mut)] pub core_config: UncheckedAccount<'info>,
    /// CHECK: token mint
    #[account(mut, address = config.token_mint)] pub token_mint: UncheckedAccount<'info>,
    /// CHECK: player token account
    #[account(mut)] pub player_token_account: UncheckedAccount<'info>,
    /// CHECK: token program
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AttemptChance<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    #[account(init, payer = player, space = 8 + PendingUpgrade::INIT_SPACE, seeds = [b"pending", asset.key().as_ref()], bump)]
    pub pending: Account<'info, PendingUpgrade>,
    /// CHECK: gear asset (read)
    pub asset: UncheckedAccount<'info>,
    /// CHECK: game_core program
    #[account(address = config.core_program)] pub core_program: UncheckedAccount<'info>,
    /// CHECK: core config PDA
    #[account(mut)] pub core_config: UncheckedAccount<'info>,
    /// CHECK: token mint
    #[account(mut, address = config.token_mint)] pub token_mint: UncheckedAccount<'info>,
    /// CHECK: player token account
    #[account(mut)] pub player_token_account: UncheckedAccount<'info>,
    /// CHECK: token program
    pub token_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ResolveChance<'info> {
    #[account(mut)] pub roll_authority: Signer<'info>,
    #[account(mut, seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    #[account(mut, close = owner, seeds = [b"pending", pending.asset.as_ref()], bump = pending.bump, has_one = asset @ ProgError::AssetMismatch)]
    pub pending: Account<'info, PendingUpgrade>,
    /// CHECK: rent destination; equals pending.owner
    #[account(mut, address = pending.owner)] pub owner: UncheckedAccount<'info>,
    /// CHECK: gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AwakenItem<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    /// CHECK: gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    // --- soul burn ---
    /// CHECK: game_resources program
    #[account(address = config.resources_program)] pub resources_program: UncheckedAccount<'info>,
    /// CHECK: Soul ResourceController PDA
    #[account(mut)] pub soul_controller: UncheckedAccount<'info>,
    /// CHECK: Soul mint
    #[account(mut, address = config.soul_mint)] pub soul_mint: UncheckedAccount<'info>,
    /// CHECK: player's Soul token account
    #[account(mut)] pub player_soul_account: UncheckedAccount<'info>,
    /// CHECK: token program
    pub token_program: UncheckedAccount<'info>,
    // --- assets apply ---
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SocketGem<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    /// CHECK: gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: gear collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    // --- gem burn (arbitrary resource mint; controller PDA enforces mint match) ---
    /// CHECK: game_resources program
    #[account(address = config.resources_program)] pub resources_program: UncheckedAccount<'info>,
    /// CHECK: gem ResourceController PDA ([b"resource", gem_mint])
    #[account(mut)] pub gem_controller: UncheckedAccount<'info>,
    /// CHECK: gem mint
    #[account(mut)] pub gem_mint: UncheckedAccount<'info>,
    /// CHECK: player's gem token account
    #[account(mut)] pub player_gem_account: UncheckedAccount<'info>,
    /// CHECK: token program
    pub token_program: UncheckedAccount<'info>,
    // --- assets apply ---
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

/// Minimal gear-attribute write (no payment / no burn), used by unsocket_gem.
#[derive(Accounts)]
pub struct GearWrite<'info> {
    #[account(mut)] pub player: Signer<'info>,
    #[account(seeds = [b"prog_config"], bump = config.config_bump)] pub config: Account<'info, ProgressionConfig>,
    /// CHECK: gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: gear collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: game_assets program
    #[account(address = config.assets_program)] pub assets_program: UncheckedAccount<'info>,
    /// CHECK: AssetsConfig PDA
    pub assets_config: UncheckedAccount<'info>,
    /// CHECK: upgrade-auth PDA (signs)
    #[account(seeds = [b"upgrade_auth"], bump = config.upgrade_auth_bump)] pub upgrade_authority: UncheckedAccount<'info>,
    /// CHECK: assets update-authority PDA
    pub assets_update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,
    #[account(mut, seeds = [b"prog_config"], bump = config.config_bump, has_one = admin @ ProgError::Unauthorized)]
    pub config: Account<'info, ProgressionConfig>,
}

// ----------------------------- state -----------------------------

#[account]
#[derive(InitSpace)]
pub struct ProgressionConfig {
    pub admin: Pubkey,
    pub roll_authority: Pubkey,
    pub xp_authority: Pubkey,
    pub core_program: Pubkey,
    pub assets_program: Pubkey,
    pub token_mint: Pubkey,
    pub base_token_cost: u64,
    pub base_success_bps: u16,
    pub success_decay_bps: u16,
    pub min_success_bps: u16,
    pub committed_seed_hash: [u8; 32],
    pub base_xp: u32,
    pub xp_per_level: u32,
    pub xp_quad: u32,
    pub max_level: u16,
    pub growth_str: u16,
    pub growth_dex: u16,
    pub growth_int: u16,
    pub growth_vit: u16,
    pub points_per_level: u16,
    pub soul_mint: Pubkey,
    pub resources_program: Pubkey,
    pub base_star_cost: u64,
    pub star_cost_growth: u64,
    pub max_stars: u8,
    pub config_bump: u8,
    pub upgrade_auth_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct PendingUpgrade {
    pub owner: Pubkey,
    pub asset: Pubkey,
    pub class: u8,
    pub quality: u8,
    pub target_level: u16,
    pub success_bps: u16,
    pub client_seed: [u8; 32],
    pub committed_seed_hash: [u8; 32],
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub enum UpgradeClass { Armor, Weapon, Accessory }
impl UpgradeClass {
    fn from_u8(v: u8) -> Result<Self> {
        match v { 0 => Ok(Self::Armor), 1 => Ok(Self::Weapon), 2 => Ok(Self::Accessory), _ => Err(ProgError::BadClass.into()) }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub enum StatKind { Strength, Agility, Intelligence, Vitality }
impl StatKind {
    fn key(&self) -> &'static str {
        match self { Self::Strength => "strength", Self::Agility => "agility", Self::Intelligence => "intelligence", Self::Vitality => "vitality" }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ProgConfigParams {
    pub roll_authority: Pubkey,
    pub xp_authority: Pubkey,
    pub core_program: Pubkey,
    pub assets_program: Pubkey,
    pub token_mint: Pubkey,
    pub base_token_cost: u64,
    pub base_success_bps: u16,
    pub success_decay_bps: u16,
    pub min_success_bps: u16,
    pub committed_seed_hash: [u8; 32],
    pub base_xp: u32,
    pub xp_per_level: u32,
    pub xp_quad: u32,
    pub max_level: u16,
    pub growth_str: u16,
    pub growth_dex: u16,
    pub growth_int: u16,
    pub growth_vit: u16,
    pub points_per_level: u16,
    pub soul_mint: Pubkey,
    pub resources_program: Pubkey,
    pub base_star_cost: u64,
    pub star_cost_growth: u64,
    pub max_stars: u8,
}

#[event] pub struct XpGranted { pub asset: Pubkey, pub amount: u64, pub level: u16, pub levels_gained: u16 }
#[event] pub struct StatAllocated { pub asset: Pubkey, pub stat: u8, pub amount: u32 }
#[event] pub struct Upgraded { pub asset: Pubkey, pub level: u16, pub success: bool, pub guaranteed: bool }
#[event] pub struct UpgradePending { pub asset: Pubkey, pub target: u16, pub success_bps: u16 }
#[event] pub struct UpgradeResolved { pub asset: Pubkey, pub success: bool, pub roll: u16, pub target: u16 }
#[event] pub struct Awakened { pub asset: Pubkey, pub stars: u8, pub soul_burned: u64 }
#[event] pub struct GemSocketed { pub asset: Pubkey, pub socket_index: u8, pub gem: Pubkey }
#[event] pub struct GemRemoved { pub asset: Pubkey, pub socket_index: u8 }

#[error_code]
pub enum ProgError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Caller is not the asset owner")] NotOwner,
    #[msg("Target within guaranteed cap; use upgrade_guaranteed")] UseGuaranteed,
    #[msg("Target exceeds guaranteed cap; use attempt_chance")] NotGuaranteed,
    #[msg("Asset missing Attributes plugin")] MissingAttributes,
    #[msg("Could not parse asset")] BadAsset,
    #[msg("Revealed seed does not match commitment")] BadReveal,
    #[msg("Pending asset mismatch")] AssetMismatch,
    #[msg("Invalid upgrade class")] BadClass,
    #[msg("Amount must be > 0")] ZeroAmount,
    #[msg("Not enough unspent points")] InsufficientPoints,
    #[msg("Item is already at max stars")] MaxStars,
    #[msg("Socket index beyond this item's socket count")] SocketOutOfRange,
    #[msg("Socket already occupied")] SocketOccupied,
    #[msg("Socket is empty")] SocketEmpty,
    #[msg("Math overflow")] MathOverflow,
}
