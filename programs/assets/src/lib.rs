// programs/assets/src/lib.rs
//
// Hero & Gear NFTs on Metaplex Core.
//
// Heroes mint FREE (no payment) and pick a CLASS at mint, which sets starting
// stats; per-class growth is applied by the progression program on level-up.
// Gear is paid in Jade. Royalties are enforced at the collection level. Only
// THIS program's [authority] PDA can mint into / mutate the collections, and it
// trusts only assets_config.upgrade_authority (the progression PDA) for stat writes.
//
// Class ids (also stored in the hero's "class" attribute; progression reads it):
//   0 Warrior · 1 Pyromancer · 2 Archer · 3 Rogue · 4 Guardian · 5 Wizard
//
// Cargo: anchor-lang = "0.31", anchor-spl = "0.31", mpl-core = "0.8".

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use mpl_core::{
    instructions::{CreateCollectionV2CpiBuilder, CreateV2CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, Creator, Plugin, PluginAuthorityPair, Royalties, RuleSet},
    ID as MPL_CORE_ID,
};

declare_id!("AsSeT1111111111111111111111111111111111111111"); // placeholder

#[program]
pub mod game_assets {
    use super::*;

    pub fn initialize_assets(ctx: Context<InitializeAssets>, params: AssetsConfigParams) -> Result<()> {
        let cfg = &mut ctx.accounts.assets_config;
        cfg.admin = ctx.accounts.admin.key();
        cfg.treasury = params.treasury;
        cfg.payment_mint = ctx.accounts.payment_mint.key();
        cfg.upgrade_authority = params.upgrade_authority;
        cfg.royalty_bps = params.royalty_bps;
        cfg.royalty_recipient = params.royalty_recipient;
        cfg.gear_price = params.gear_price; // heroes are free
        cfg.heroes_collection = Pubkey::default();
        cfg.gear_collection = Pubkey::default();
        cfg.config_bump = ctx.bumps.assets_config;
        cfg.authority_bump = ctx.bumps.update_authority;
        Ok(())
    }

    pub fn create_collection(ctx: Context<CreateCollection>, kind: CollectionKind, name: String, uri: String) -> Result<()> {
        let cfg = &mut ctx.accounts.assets_config;
        let royalties = Plugin::Royalties(Royalties {
            basis_points: cfg.royalty_bps,
            creators: vec![Creator { address: cfg.royalty_recipient, percentage: 100 }],
            rule_set: RuleSet::None,
        });
        CreateCollectionV2CpiBuilder::new(&ctx.accounts.mpl_core_program)
            .collection(&ctx.accounts.collection.to_account_info())
            .update_authority(Some(&ctx.accounts.update_authority.to_account_info()))
            .payer(&ctx.accounts.admin.to_account_info())
            .system_program(&ctx.accounts.system_program.to_account_info())
            .name(name).uri(uri)
            .plugins(vec![PluginAuthorityPair { plugin: royalties, authority: None }])
            .invoke()?;
        match kind {
            CollectionKind::Heroes => cfg.heroes_collection = ctx.accounts.collection.key(),
            CollectionKind::Gear => cfg.gear_collection = ctx.accounts.collection.key(),
        }
        Ok(())
    }

    /// FREE hero mint. Class sets starting stats; level/exp/unspent start at base.
    pub fn mint_hero(ctx: Context<MintHero>, name: String, uri: String, class: HeroClass) -> Result<()> {
        let (s, a, i, v) = class.starting_stats();
        let attributes = Plugin::Attributes(Attributes {
            attribute_list: vec![
                attr("class", &(class as u8).to_string()),
                attr("level", "1"), attr("exp", "0"), attr("soul_power", "0"),
                attr("strength", &s.to_string()), attr("agility", &a.to_string()),
                attr("intelligence", &i.to_string()), attr("vitality", &v.to_string()),
                attr("unspent", "0"),
            ],
        });
        create_asset(
            &ctx.accounts.mpl_core_program, &ctx.accounts.asset.to_account_info(),
            &ctx.accounts.collection.to_account_info(), &ctx.accounts.update_authority.to_account_info(),
            &ctx.accounts.buyer.to_account_info(), &ctx.accounts.buyer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(), ctx.accounts.assets_config.authority_bump,
            name, uri, attributes,
        )
    }

    /// PAID gear mint (Jade -> treasury). `affixes` are the slot's base stats.
    pub fn mint_gear(ctx: Context<MintGear>, name: String, uri: String, params: GearMintParams) -> Result<()> {
        take_payment(&ctx, ctx.accounts.assets_config.gear_price)?;
        let mut list = vec![
            attr("slot", &params.slot), attr("item_level", &params.item_level.to_string()),
            attr("quality", &params.quality.to_string()),
            attr("plus", "0"), attr("stars", "1"), attr("awaken", "0"),
        ];
        for a in params.affixes.iter() {
            list.push(Attribute { key: a.key.clone(), value: a.value.clone() });
        }
        create_asset(
            &ctx.accounts.mpl_core_program, &ctx.accounts.asset.to_account_info(),
            &ctx.accounts.collection.to_account_info(), &ctx.accounts.update_authority.to_account_info(),
            &ctx.accounts.buyer.to_account_info(), &ctx.accounts.buyer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(), ctx.accounts.assets_config.authority_bump,
            name, uri, Plugin::Attributes(Attributes { attribute_list: list }),
        )
    }

    /// Guarded stat write for heroes + gear (only the progression PDA may call).
    pub fn set_asset_attributes(ctx: Context<SetAssetAttributes>, attributes: Vec<AttrArg>) -> Result<()> {
        require_keys_eq!(ctx.accounts.upgrade_authority.key(), ctx.accounts.assets_config.upgrade_authority, AssetError::Unauthorized);
        let attribute_list = attributes.into_iter().map(|a| Attribute { key: a.key, value: a.value }).collect();
        let bump = ctx.accounts.assets_config.authority_bump;
        let signer_seeds: &[&[&[u8]]] = &[&[b"authority", &[bump]]];
        UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.payer.to_account_info())
            .authority(Some(&ctx.accounts.update_authority.to_account_info()))
            .system_program(&ctx.accounts.system_program.to_account_info())
            .plugin(Plugin::Attributes(Attributes { attribute_list }))
            .invoke_signed(signer_seeds)?;
        Ok(())
    }
}

// --------------------------- helpers ---------------------------

fn attr(key: &str, value: &str) -> Attribute { Attribute { key: key.to_string(), value: value.to_string() } }

#[allow(clippy::too_many_arguments)]
fn create_asset<'info>(
    mpl_core: &AccountInfo<'info>, asset: &AccountInfo<'info>, collection: &AccountInfo<'info>,
    update_authority: &AccountInfo<'info>, payer: &AccountInfo<'info>, owner: &AccountInfo<'info>,
    system: &AccountInfo<'info>, authority_bump: u8, name: String, uri: String, plugin: Plugin,
) -> Result<()> {
    let seeds: &[&[&[u8]]] = &[&[b"authority", &[authority_bump]]];
    CreateV2CpiBuilder::new(mpl_core)
        .asset(asset).collection(Some(collection)).authority(Some(update_authority))
        .payer(payer).owner(Some(owner)).system_program(system)
        .name(name).uri(uri)
        .plugins(vec![PluginAuthorityPair { plugin, authority: None }])
        .invoke_signed(seeds)?;
    Ok(())
}

fn take_payment(ctx: &Context<MintGear>, amount: u64) -> Result<()> {
    if amount == 0 { return Ok(()); }
    token_interface::transfer_checked(
        CpiContext::new(ctx.accounts.token_program.to_account_info(), TransferChecked {
            from: ctx.accounts.buyer_token_account.to_account_info(),
            mint: ctx.accounts.payment_mint.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.buyer.to_account_info(),
        }),
        amount, ctx.accounts.payment_mint.decimals,
    )
}

// --------------------------- accounts ---------------------------

#[derive(Accounts)]
pub struct InitializeAssets<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + AssetsConfig::INIT_SPACE, seeds = [b"assets_config"], bump)]
    pub assets_config: Account<'info, AssetsConfig>,
    /// CHECK: collection update-authority PDA
    #[account(seeds = [b"authority"], bump)] pub update_authority: UncheckedAccount<'info>,
    pub payment_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateCollection<'info> {
    #[account(mut, has_one = admin @ AssetError::Unauthorized, seeds = [b"assets_config"], bump = assets_config.config_bump)]
    pub assets_config: Account<'info, AssetsConfig>,
    #[account(mut)] pub admin: Signer<'info>,
    #[account(mut)] pub collection: Signer<'info>,
    /// CHECK: update-authority PDA
    #[account(seeds = [b"authority"], bump = assets_config.authority_bump)] pub update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintHero<'info> {
    #[account(seeds = [b"assets_config"], bump = assets_config.config_bump)]
    pub assets_config: Account<'info, AssetsConfig>,
    #[account(mut)] pub buyer: Signer<'info>,
    #[account(mut)] pub asset: Signer<'info>,
    /// CHECK: heroes collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: update-authority PDA
    #[account(seeds = [b"authority"], bump = assets_config.authority_bump)] pub update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintGear<'info> {
    #[account(seeds = [b"assets_config"], bump = assets_config.config_bump)]
    pub assets_config: Account<'info, AssetsConfig>,
    #[account(mut)] pub buyer: Signer<'info>,
    #[account(mut)] pub asset: Signer<'info>,
    /// CHECK: gear collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: update-authority PDA
    #[account(seeds = [b"authority"], bump = assets_config.authority_bump)] pub update_authority: UncheckedAccount<'info>,
    #[account(address = assets_config.payment_mint)] pub payment_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)] pub buyer_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = treasury_token_account.owner == assets_config.treasury @ AssetError::BadTreasury)]
    pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetAssetAttributes<'info> {
    #[account(seeds = [b"assets_config"], bump = assets_config.config_bump)]
    pub assets_config: Account<'info, AssetsConfig>,
    pub upgrade_authority: Signer<'info>,
    #[account(mut)] pub payer: Signer<'info>,
    /// CHECK: hero or gear asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: heroes OR gear collection
    #[account(mut, constraint =
        collection.key() == assets_config.heroes_collection || collection.key() == assets_config.gear_collection
        @ AssetError::BadCollection)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: update-authority PDA
    #[account(seeds = [b"authority"], bump = assets_config.authority_bump)] pub update_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// ---------------------------- state ----------------------------

#[account]
#[derive(InitSpace)]
pub struct AssetsConfig {
    pub admin: Pubkey,
    pub treasury: Pubkey,
    pub payment_mint: Pubkey,
    pub upgrade_authority: Pubkey,
    pub royalty_recipient: Pubkey,
    pub royalty_bps: u16,
    pub gear_price: u64,
    pub heroes_collection: Pubkey,
    pub gear_collection: Pubkey,
    pub config_bump: u8,
    pub authority_bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AssetsConfigParams {
    pub treasury: Pubkey,
    pub upgrade_authority: Pubkey,
    pub royalty_recipient: Pubkey,
    pub royalty_bps: u16,
    pub gear_price: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub enum CollectionKind { Heroes, Gear }

/// Class chosen at hero mint. `as u8` discriminant is stored in "class".
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub enum HeroClass { Warrior, Pyromancer, Archer, Rogue, Guardian, Wizard }
impl HeroClass {
    /// (strength, agility, intelligence, vitality) starting stats.
    pub fn starting_stats(&self) -> (u16, u16, u16, u16) {
        match self {
            HeroClass::Warrior => (12, 6, 4, 10),
            HeroClass::Pyromancer => (4, 5, 12, 7),
            HeroClass::Archer => (6, 12, 5, 7),
            HeroClass::Rogue => (8, 12, 4, 6),
            HeroClass::Guardian => (8, 4, 4, 14),
            HeroClass::Wizard => (3, 5, 14, 6),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct GearMintParams {
    pub slot: String,
    pub item_level: u16,
    pub quality: u8,
    pub affixes: Vec<AttrArg>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AttrArg { pub key: String, pub value: String }

#[error_code]
pub enum AssetError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Treasury token account owner mismatch")] BadTreasury,
    #[msg("Collection not recognized")] BadCollection,
}
