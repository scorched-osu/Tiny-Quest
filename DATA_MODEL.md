# Game Data Model & On-Chain Mapping

Derived from the reference screenshots. Genre: idle/auto-battler ARPG with an
equipment-driven power economy. We build our *own* art, names, and assets — the
reference informs mechanics and schema only.

---

## 1. Currencies

| In-game | Reference | Role | On-chain representation |
|---|---|---|---|
| **Jade** | green gem (2,094,819) | main soft currency; enhancement, crafting, market | **Native utility token** (SPL/Token-2022) — the tradeable DeFi token |
| **Soul** | blue drop (17,865,124), drops in combat | mid currency for awakening / skills | **Fungible resource token** (Token-2022); faucet = combat (backend grant), sink = awakening |
| **Diamonds** | premium gem (0) | premium store | Off-chain or direct SOL/USDC purchase — NOT minted as a token |
| **Exp** | per-kill | hero leveling | Off-chain accrual → `grant_xp` |

Faucet/sink discipline still applies: Jade is minted under the per-epoch cap;
Soul should have its own capped emission + strong awakening sink.

---

## 2. Hero (Core NFT — Heroes collection)

Identity + live stats stored as Attributes plugin keys:

| Key | Type | Notes |
|---|---|---|
| `level` | u16 | from Exp; drives base stats |
| `soul_power` | u16 | power rating shown beside level (derived; can be cached) |
| `exp` | u64 | current XP toward next level |
| `hp`, `mp` | u32 | max pools |
| `attack_min`, `attack_max` | u32 | weapon-driven attack range |
| `defence` | u32 | |
| `crit_bps` | u16 | crit chance, basis points (1800 = 18%) |
| `strength`, `agility`, `intelligence`, `vitality` | u32 | primary attributes |
| `unspent` | u32 | allocatable points from leveling |

### Classes (chosen at free mint)
Hero is a **free-to-mint NFT**. Class is picked at mint, stored in the `class`
attribute (0 Warrior · 1 Pyromancer · 2 Archer · 3 Rogue · 4 Guardian · 5 Wizard),
and sets starting stats + per-level growth (`game_assets::HeroClass` +
`progression::class_growth`):

| Class | start STR/AGI/INT/VIT | growth /level |
|---|---|---|
| Warrior | 12/6/4/10 | +2/+1/+0/+2 |
| Pyromancer | 4/5/12/7 | +0/+1/+3/+1 |
| Archer | 6/12/5/7 | +1/+3/+0/+1 |
| Rogue | 8/12/4/6 | +2/+2/+0/+1 |
| Guardian | 8/4/4/14 | +1/+0/+0/+4 |
| Wizard | 3/5/14/6 | +0/+1/+3/+1 |

Weapon-type **proficiency** bars (melee/ranged/magic %) seen in the reference are
a separate soft-affinity layer — still TODO.

---

## 3. Item (Core NFT — Gear collection)

Each item is one Core asset. Display string = `+{plus}|{stars}` (the `|stars`
hidden when stars <= 1, e.g. a weapon shows just `+7`).

| Key | Type | Notes |
|---|---|---|
| `slot` | string | weapon, offhand, helm, chest, legs, gloves, cloak, belt, amulet, ring, pet |
| `item_level` | u16 | required hero level (Lv.40 / Lv.49 / Lv.50 in refs) |
| `quality` | u8 | 0 Common … 3 Epic, 4 Legendary, 5 Unique, 6 Mythic |
| `plus` | u16 | **enhancement track** (+N) — built |
| `stars` | u8 | **awakening track** (1–10 pink stars) — ✅ built |
| `awaken` | u8 | winged-shield number (sub-level within a star tier) |
| `item_power` | u32 | starburst score (44 / 27 / 794) — derived gear score |
| affixes | varies | subset below, depends on slot |

### Affix pool (item stats seen in refs)
`atk_min`, `atk_max`, `defence`, `hp`, `mp`, `hp_regain`, `mp_regain`,
`strength`, `agility`, `intelligence`, `vitality`, `crit_bps`, `crit_dmg_bps`,
`penetrate`, `precision`, `max_attack`, `damage_increase_bps`.

Stat display convention: `total(base+bonus)` e.g. `Intelligence 4(3+1)`,
`DamageIncrease 7%(2+5)` — i.e. stored base + enhancement/star bonus, summed in UI.

### Gem sockets ✅ built (progression program)
Sockets are stored inline on the gear as `socket_0`, `socket_1`, `socket_2`
attributes. An empty/absent value = free slot; a filled value = the base58 of the
socketed **gem mint**. Socket count derives from quality (no stored capacity):

| Quality | Sockets |
|---|---|
| Common | 0 |
| Uncommon · Rare | 1 |
| Epic · Legendary | 2 |
| Unique · Mythic+ | 3 |

Gems are **0-decimal fungible resource mints** (one token = one gem), each mint
encoding a type+tier (e.g. Ruby-I = +ATK, Sapphire-II = +HP). `socket_gem` burns
one gem via the resource controller (which enforces the mint↔controller match) and
records the mint id; `unsocket_gem` frees a slot (gem consumed, not refunded). The
gem's actual stat contribution is resolved **off-chain** from a gem registry keyed
by mint — on-chain only records *which* gem sits where, same lean-data rule as
affixes/plus/stars.

---

## 4. The two upgrade tracks

| Track | Currency | Mechanic | Status |
|---|---|---|---|
| **Enhancement (+N)** | Jade | guaranteed to +4 (armor) / +6 (weapon) for ≤Rare; chance above cap and for Epic+/accessories | ✅ built (progression program) |
| **Awakening (stars)** | Soul | raises star count 1→10, unlocking affix tiers; winged-shield `awaken` sub-levels | ✅ built (progression program) |

Both mutate the same Core asset via `set_asset_attributes` (progression's
upgrade-auth PDA). Awakening adds: a star-cost curve, an optional
duplicate-burn requirement, and per-star affix unlocks (curve + Soul sink ✅ built).

---

## 5. Required changes to existing programs

- **assets**: `mint_gear` should accept the real affix set + `slot`, `item_level`,
  `quality`, `stars=1`, `plus=0`. `mint_hero` stat keys shift to the hero schema above.
- **progression**:
  - rename/repoint gear bonus keys: weapon → `atk_max`/`atk_min`, armor → `defence`,
    accessory → affix-specific (`intelligence`, `mp_regain`, `crit_bps`, …) instead of
    the placeholder `atk_bonus/def_bonus/power_bonus`.
  - hero `grant_xp` writes the new hero stat keys; growth vectors become per-archetype config.
  - add the **awakening** instructions (`awaken_item`, star-cost curve, Soul sink).
- **core**: add a second controlled mint = **Soul** resource token (or a generic
  "resource controller" supporting N resource mints with per-mint epoch caps).

---

## 6. Systems still to scope (seen in refs)

Trade (marketplace), Arena (PvP), Social (guild), Craft, Skill tree, Titles,
Treasure (gacha chests), daily/seasonal events, pets as combat companions.
