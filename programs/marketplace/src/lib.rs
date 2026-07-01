// programs/marketplace/src/lib.rs
//
// In-game marketplace (the "Trade" tab). Escrow model:
//   list   -> Core asset transferred seller -> escrow PDA, Listing opened
//   buy    -> buyer pays Jade; split 4 ways; asset transferred escrow -> buyer
//   delist -> asset returned escrow -> seller
//
// Buy-time split of `price` (all debited from the buyer in the native token):
//   royalty      = price * royalty_bps   -> royalty_recipient   (creator)
//   fee          = price * fee_bps        (protocol take)
//     burn       = fee   * burn_share_bps -> burned (deflationary sink)
//     treasury   = fee - burn             -> treasury
//   seller       = price - royalty - fee  -> seller
//
// Royalty is paid by OUR program from proceeds (with Core RuleSet::None,
// enforcement is at the marketplace layer — which we control). The burn is a
// CPI into game_core::burn_sink with the buyer as the burning owner.
//
// Cargo: anchor-lang/anchor-spl = "0.31", mpl-core = "0.8",
//        game_core = { path="../core", features=["cpi"] }.

use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    self, Mint, TokenAccount, TokenInterface, TransferChecked,
};
use mpl_core::{instructions::TransferV1CpiBuilder, ID as MPL_CORE_ID};

use game_core::cpi as core_cpi;
use game_core::cpi::accounts::BurnSink as CoreBurnSink;

declare_id!("MrKt1111111111111111111111111111111111111111"); // placeholder

const MAX_BPS: u16 = 10_000;

#[program]
pub mod game_marketplace {
    use super::*;

    pub fn init_market(ctx: Context<InitMarket>, params: MarketParams) -> Result<()> {
        require!(params.fee_bps <= MAX_BPS, MarketError::InvalidBps);
        require!(params.royalty_bps <= MAX_BPS, MarketError::InvalidBps);
        require!(params.burn_share_bps <= MAX_BPS, MarketError::InvalidBps);

        let m = &mut ctx.accounts.market;
        m.admin = ctx.accounts.admin.key();
        m.payment_mint = ctx.accounts.payment_mint.key();
        m.treasury = params.treasury;
        m.royalty_recipient = params.royalty_recipient;
        m.core_program = params.core_program;
        m.fee_bps = params.fee_bps;
        m.royalty_bps = params.royalty_bps;
        m.burn_share_bps = params.burn_share_bps;
        m.bump = ctx.bumps.market;
        m.escrow_bump = ctx.bumps.escrow_authority;
        Ok(())
    }

    /// List a Core asset for sale. Transfers it into the escrow PDA.
    pub fn list(ctx: Context<ListAsset>, price: u64) -> Result<()> {
        require!(price > 0, MarketError::ZeroPrice);

        let l = &mut ctx.accounts.listing;
        l.seller = ctx.accounts.seller.key();
        l.asset = ctx.accounts.asset.key();
        l.collection = ctx.accounts.collection.key();
        l.price = price;
        l.bump = ctx.bumps.listing;

        TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.seller.to_account_info())
            .authority(Some(&ctx.accounts.seller.to_account_info()))
            .new_owner(&ctx.accounts.escrow_authority.to_account_info())
            .system_program(Some(&ctx.accounts.system_program.to_account_info()))
            .invoke()?;

        emit!(Listed { asset: l.asset, seller: l.seller, price });
        Ok(())
    }

    pub fn update_price(ctx: Context<ManageListing>, new_price: u64) -> Result<()> {
        require!(new_price > 0, MarketError::ZeroPrice);
        ctx.accounts.listing.price = new_price;
        emit!(PriceUpdated { asset: ctx.accounts.listing.asset, price: new_price });
        Ok(())
    }

    /// Seller reclaims an unsold asset.
    pub fn delist(ctx: Context<ManageListing>) -> Result<()> {
        let seeds: &[&[u8]] = &[b"escrow", &[ctx.accounts.market.escrow_bump]];
        TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.seller.to_account_info())
            .authority(Some(&ctx.accounts.escrow_authority.to_account_info()))
            .new_owner(&ctx.accounts.seller.to_account_info())
            .system_program(Some(&ctx.accounts.system_program.to_account_info()))
            .invoke_signed(&[seeds])?;

        emit!(Delisted { asset: ctx.accounts.listing.asset });
        Ok(()) // listing closed via `close = seller`
    }

    /// Purchase. Splits payment, burns the sink cut, releases the asset.
    pub fn buy(ctx: Context<Buy>) -> Result<()> {
        let m = &ctx.accounts.market;
        let price = ctx.accounts.listing.price;
        let decimals = ctx.accounts.payment_mint.decimals;

        let royalty = bps_of(price, m.royalty_bps)?;
        let fee = bps_of(price, m.fee_bps)?;
        let burn = bps_of(fee, m.burn_share_bps)?;
        let treasury_amt = fee.checked_sub(burn).ok_or(MarketError::MathOverflow)?;
        let seller_amt = price
            .checked_sub(royalty).ok_or(MarketError::MathOverflow)?
            .checked_sub(fee).ok_or(MarketError::MathOverflow)?;

        // --- payments, all from buyer (buyer signs) ---
        pay(&ctx, ctx.accounts.royalty_token_account.to_account_info(), royalty, decimals)?;
        pay(&ctx, ctx.accounts.treasury_token_account.to_account_info(), treasury_amt, decimals)?;
        pay(&ctx, ctx.accounts.seller_token_account.to_account_info(), seller_amt, decimals)?;

        if burn > 0 {
            core_cpi::burn_sink(
                CpiContext::new(ctx.accounts.core_program.to_account_info(), CoreBurnSink {
                    owner: ctx.accounts.buyer.to_account_info(),
                    config: ctx.accounts.core_config.to_account_info(),
                    token_mint: ctx.accounts.payment_mint.to_account_info(),
                    from: ctx.accounts.buyer_token_account.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                }),
                burn,
            )?;
        }

        // --- release asset escrow -> buyer ---
        let seeds: &[&[u8]] = &[b"escrow", &[m.escrow_bump]];
        TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .payer(&ctx.accounts.buyer.to_account_info())
            .authority(Some(&ctx.accounts.escrow_authority.to_account_info()))
            .new_owner(&ctx.accounts.buyer.to_account_info())
            .system_program(Some(&ctx.accounts.system_program.to_account_info()))
            .invoke_signed(&[seeds])?;

        emit!(Sold {
            asset: ctx.accounts.listing.asset,
            seller: ctx.accounts.listing.seller,
            buyer: ctx.accounts.buyer.key(),
            price, royalty, fee, burn,
        });
        Ok(()) // listing closed via `close = seller`
    }
}

// --------------------------- helpers ---------------------------

fn bps_of(amount: u64, bps: u16) -> Result<u64> {
    Ok((amount as u128)
        .checked_mul(bps as u128).ok_or(MarketError::MathOverflow)?
        .checked_div(MAX_BPS as u128).ok_or(MarketError::MathOverflow)? as u64)
}

fn pay(ctx: &Context<Buy>, to: AccountInfo, amount: u64, decimals: u8) -> Result<()> {
    if amount == 0 { return Ok(()); }
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.buyer_token_account.to_account_info(),
                mint: ctx.accounts.payment_mint.to_account_info(),
                to,
                authority: ctx.accounts.buyer.to_account_info(),
            },
        ),
        amount,
        decimals,
    )
}

// --------------------------- accounts ---------------------------

#[derive(Accounts)]
pub struct InitMarket<'info> {
    #[account(mut)] pub admin: Signer<'info>,
    #[account(init, payer = admin, space = 8 + Marketplace::INIT_SPACE, seeds = [b"market"], bump)]
    pub market: Account<'info, Marketplace>,
    /// CHECK: escrow PDA that holds listed assets
    #[account(seeds = [b"escrow"], bump)] pub escrow_authority: UncheckedAccount<'info>,
    pub payment_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ListAsset<'info> {
    #[account(seeds = [b"market"], bump = market.bump)] pub market: Account<'info, Marketplace>,
    #[account(mut)] pub seller: Signer<'info>,
    #[account(init, payer = seller, space = 8 + Listing::INIT_SPACE, seeds = [b"listing", asset.key().as_ref()], bump)]
    pub listing: Account<'info, Listing>,
    /// CHECK: Core asset being listed
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: the asset's collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: escrow PDA (new owner)
    #[account(seeds = [b"escrow"], bump = market.escrow_bump)] pub escrow_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ManageListing<'info> {
    #[account(seeds = [b"market"], bump = market.bump)] pub market: Account<'info, Marketplace>,
    #[account(mut)] pub seller: Signer<'info>,
    #[account(mut, close = seller, seeds = [b"listing", listing.asset.as_ref()], bump = listing.bump,
        has_one = seller @ MarketError::Unauthorized)]
    pub listing: Account<'info, Listing>,
    /// CHECK: Core asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: escrow PDA
    #[account(seeds = [b"escrow"], bump = market.escrow_bump)] pub escrow_authority: UncheckedAccount<'info>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(seeds = [b"market"], bump = market.bump)] pub market: Account<'info, Marketplace>,
    #[account(mut)] pub buyer: Signer<'info>,
    #[account(mut, close = seller, seeds = [b"listing", listing.asset.as_ref()], bump = listing.bump)]
    pub listing: Account<'info, Listing>,
    /// CHECK: seller (rent + proceeds recipient); validated against listing
    #[account(mut, address = listing.seller)] pub seller: UncheckedAccount<'info>,

    /// CHECK: Core asset
    #[account(mut)] pub asset: UncheckedAccount<'info>,
    /// CHECK: collection
    #[account(mut)] pub collection: UncheckedAccount<'info>,
    /// CHECK: escrow PDA (current owner)
    #[account(seeds = [b"escrow"], bump = market.escrow_bump)] pub escrow_authority: UncheckedAccount<'info>,

    // --- payment ---
    #[account(address = market.payment_mint)] pub payment_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)] pub buyer_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = seller_token_account.owner == listing.seller @ MarketError::BadRecipient)]
    pub seller_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = treasury_token_account.owner == market.treasury @ MarketError::BadRecipient)]
    pub treasury_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut, constraint = royalty_token_account.owner == market.royalty_recipient @ MarketError::BadRecipient)]
    pub royalty_token_account: InterfaceAccount<'info, TokenAccount>,

    // --- burn via core ---
    /// CHECK: game_core program
    #[account(address = market.core_program)] pub core_program: UncheckedAccount<'info>,
    /// CHECK: core Config PDA
    #[account(mut)] pub core_config: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    /// CHECK: Metaplex Core program
    #[account(address = MPL_CORE_ID)] pub mpl_core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// ---------------------------- state ----------------------------

#[account]
#[derive(InitSpace)]
pub struct Marketplace {
    pub admin: Pubkey,
    pub payment_mint: Pubkey,
    pub treasury: Pubkey,
    pub royalty_recipient: Pubkey,
    pub core_program: Pubkey,
    pub fee_bps: u16,
    pub royalty_bps: u16,
    pub burn_share_bps: u16,
    pub bump: u8,
    pub escrow_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Listing {
    pub seller: Pubkey,
    pub asset: Pubkey,
    pub collection: Pubkey,
    pub price: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct MarketParams {
    pub treasury: Pubkey,
    pub royalty_recipient: Pubkey,
    pub core_program: Pubkey,
    pub fee_bps: u16,
    pub royalty_bps: u16,
    pub burn_share_bps: u16,
}

#[event] pub struct Listed { pub asset: Pubkey, pub seller: Pubkey, pub price: u64 }
#[event] pub struct PriceUpdated { pub asset: Pubkey, pub price: u64 }
#[event] pub struct Delisted { pub asset: Pubkey }
#[event] pub struct Sold { pub asset: Pubkey, pub seller: Pubkey, pub buyer: Pubkey, pub price: u64, pub royalty: u64, pub fee: u64, pub burn: u64 }

#[error_code]
pub enum MarketError {
    #[msg("Caller is not authorized")] Unauthorized,
    #[msg("Basis points must be <= 10000")] InvalidBps,
    #[msg("Price must be > 0")] ZeroPrice,
    #[msg("Token account owner mismatch")] BadRecipient,
    #[msg("Math overflow")] MathOverflow,
}
