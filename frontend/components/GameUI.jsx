"use client";
import React, { useState, useEffect, useRef } from "react";
import { Gem, Droplet, Swords, Shield, Sparkles, Star, Plus, Zap, Crosshair, Heart, FlaskConical } from "lucide-react";

// ───────────────────────────────────────────────────────────────────────────
// Playable slice. Client-side simulation that mirrors the on-chain rules.
// Each action notes the program instruction it maps to in production.
//   enhance(guaranteed)  -> progression.upgrade_guaranteed
//   enhance(chance)      -> progression.attempt_chance + backend resolve_chance
//   awaken               -> progression.awaken_item   (Soul burned)
//   claim                -> backend /battle/claim -> grant_xp + mint_resource + mint_reward
//   allocate             -> progression.allocate_stat
// ───────────────────────────────────────────────────────────────────────────

const C = {
  bg: "#e7e4f2", panel: "#ffffff", ink: "#3a3550", sub: "#8a85a3",
  jade: "#34c759", soul: "#36a0ff", gem: "#a855f7", magenta: "#ff3da5",
  gold: "#ffc043", danger: "#ff4d5e", frost: "#dfe9ff",
  cardA: "#3b2b53", cardB: "#1d1530",
};
const QUALITY = ["Common", "Uncommon", "Rare", "Epic", "Legendary", "Unique", "Mythic"];
const CLASSES = [
  { id: 0, name: "Warrior", icon: "⚔️", start: [12, 6, 4, 10], blurb: "STR/VIT bruiser" },
  { id: 1, name: "Pyromancer", icon: "🔥", start: [4, 5, 12, 7], blurb: "INT fire burst" },
  { id: 2, name: "Archer", icon: "🏹", start: [6, 12, 5, 7], blurb: "AGI ranged" },
  { id: 3, name: "Rogue", icon: "🗡️", start: [8, 12, 4, 6], blurb: "STR/AGI assassin" },
  { id: 4, name: "Guardian", icon: "🛡️", start: [8, 4, 4, 14], blurb: "VIT tank" },
  { id: 5, name: "Wizard", icon: "🪄", start: [3, 5, 14, 6], blurb: "INT arcane" },
];
const QCOLOR = ["#9aa0b4", "#46c98b", "#3a86ff", "#a855f7", "#ff8c1a", "#ff3da5", "#ff2d2d"];
const px = { fontFamily: "'Pixelify Sans', monospace" };
const round = { fontFamily: "'Nunito', system-ui, sans-serif" };

const SEED_ITEMS = [
  { id: 1, name: "Hydra Wand", slot: "weapon", q: 3, plus: 7, stars: 1, lvl: 50, icon: "🪄", base: { atk_min: 55, atk_max: 187, intelligence: 12, mp_regain: 7 } },
  { id: 2, name: "Spider Charm", slot: "amulet", q: 3, plus: 0, stars: 4, lvl: 40, icon: "📿", base: { damage_increase_bps: 200, max_attack: 2, precision: 1, crit_bps: 100 } },
  { id: 3, name: "Ironweave Vest", slot: "chest", q: 2, plus: 5, stars: 1, lvl: 50, icon: "🦺", base: { defence: 16, hp: 60, strength: 8, hp_regain: 7 } },
  { id: 4, name: "Gladiator Cloak", slot: "cloak", q: 3, plus: 2, stars: 2, lvl: 51, icon: "🧥", base: { penetrate: 2, crit_bps: 300, crit_dmg_bps: 400, max_attack: 15 } },
  { id: 5, name: "Ghost Belt", slot: "belt", q: 1, plus: 1, stars: 1, lvl: 49, icon: "🎗️", base: { intelligence: 3, mp_regain: 1 } },
  { id: 6, name: "Iron Gauntlets", slot: "gloves", q: 1, plus: 3, stars: 1, lvl: 18, icon: "🧤", base: { defence: 3, strength: 2, vitality: 3, max_attack: 18 } },
  { id: 7, name: "Verdant Crown", slot: "helm", q: 4, plus: 4, stars: 1, lvl: 48, icon: "👑", base: { defence: 9, intelligence: 6, hp: 40 } },
  { id: 8, name: "Soul Ring", slot: "ring", q: 5, plus: 1, stars: 3, lvl: 52, icon: "💍", base: { crit_bps: 250, intelligence: 5 } },
  // alternatives / empty-slot fillers — equipping these is a real choice
  { id: 9, name: "Ember Staff", slot: "weapon", q: 4, plus: 2, stars: 1, lvl: 52, icon: "🔮", base: { atk_min: 80, atk_max: 150, intelligence: 20, crit_bps: 150 } },
  { id: 10, name: "Titan Greaves", slot: "legs", q: 2, plus: 3, stars: 1, lvl: 46, icon: "🥾", base: { defence: 14, hp: 50, vitality: 6 } },
  { id: 11, name: "Sprite Pet", slot: "pet", q: 3, plus: 0, stars: 2, lvl: 40, icon: "🧚", base: { max_attack: 25, crit_bps: 120 } },
];

// NPC market listings — buyable with Jade (mirrors marketplace.buy)
const MARKET_SEED = [
  { id: 101, name: "Storm Blade", slot: "weapon", q: 4, plus: 3, stars: 1, lvl: 55, icon: "⚔️", base: { atk_min: 90, atk_max: 240, agility: 10, crit_bps: 200 }, price: 8500 },
  { id: 102, name: "Aegis Plate", slot: "chest", q: 3, plus: 4, stars: 2, lvl: 54, icon: "🛡️", base: { defence: 28, hp: 120, vitality: 10 }, price: 6200 },
  { id: 103, name: "Phoenix Ring", slot: "ring", q: 5, plus: 2, stars: 4, lvl: 56, icon: "💍", base: { crit_bps: 400, crit_dmg_bps: 600, intelligence: 8 }, price: 12800 },
  { id: 104, name: "Swift Boots", slot: "legs", q: 3, plus: 5, stars: 1, lvl: 50, icon: "🥾", base: { defence: 18, agility: 14, hp: 70 }, price: 4300 },
];
// marketplace 4-way split (mirrors marketplace program config)
const FEE_BPS = 500, ROYALTY_BPS = 500, BURN_SHARE = 0.30;
const splitOf = (price) => {
  const fee = Math.round((price * FEE_BPS) / 10000);
  const royalty = Math.round((price * ROYALTY_BPS) / 10000);
  const burn = Math.round(fee * BURN_SHARE);
  const treasury = fee - burn;
  const seller = price - fee - royalty;
  return { fee, royalty, burn, treasury, seller };
};

// ── rules (mirror progression program) ──
const ARMOR = ["helm", "chest", "legs", "gloves", "belt"];
const ACCESSORY = ["ring", "amulet", "cloak", "pet"];
const guaranteedCap = (slot, q) => (q >= 3 ? 0 : ARMOR.includes(slot) ? 4 : slot === "weapon" ? 6 : 0);
const enhanceCost = (plus) => 10 * (plus + 1);
const successBps = (over) => Math.max(500, 7500 - 750 * (over - 1));
const starCost = (stars) => 100 + 50 * (stars - 1);
const xpForLevel = (L) => 100 + 50 * (L - 1) + 10 * (L - 1) * (L - 1);
const affixLabel = {
  atk_min: "Attack", atk_max: "Attack", defence: "Defence", hp: "HP", mp: "MP",
  hp_regain: "HP Regain", mp_regain: "MP Regain", strength: "Strength", agility: "Agility",
  intelligence: "Intelligence", vitality: "Vitality", crit_bps: "Critical", crit_dmg_bps: "Crit Damage",
  penetrate: "Penetrate", precision: "Precision", max_attack: "MaxAttack", damage_increase_bps: "DamageIncrease",
};
const isPct = (k) => k.endsWith("_bps");
// total(base+bonus): bonus scales with plus and stars
const bonusOf = (b, plus, stars) => Math.round(b * (0.15 * plus + 0.1 * (stars - 1)));
const fmtVal = (k, v) => (isPct(k) ? `${(v / 100).toFixed(0)}%` : `${v}`);

// ── auto-battler (mirrors the idle combat loop; drops feed grant_xp / mint_resource) ──
const MONSTERS = [
  { name: "Cave Bat", icon: "🦇" }, { name: "Goblin", icon: "👺" },
  { name: "Slime King", icon: "🟢" }, { name: "Skeleton", icon: "💀" },
  { name: "Dark Mage", icon: "🧟" }, { name: "Frost Wyrm", icon: "🐉" },
  { name: "Demon Lord", icon: "👹" },
];
const enemyForStage = (stage) => {
  const m = MONSTERS[(stage - 1) % MONSTERS.length];
  const boss = stage % 5 === 0;
  const hpMax = Math.round(320 * Math.pow(1.16, stage - 1)) * (boss ? 3 : 1);
  const atk = Math.round(16 * Math.pow(1.11, stage - 1)) * (boss ? 2 : 1);
  return { name: boss ? `${m.name} ★` : m.name, icon: m.icon, hp: hpMax, hpMax, atk, boss };
};
// active skills (maps to the skills program's per-hero skill book); auto-cast on
// cooldown during the battle loop. cd is in combat ticks (~650ms each).
const SKILLS = [
  { name: "Fireball", icon: "🔥", type: "dmg", mult: 1.2, cd: 4, color: "#ff8c1a" },
  { name: "Mend", icon: "✨", type: "heal", amt: 0.22, cd: 9, color: "#34c759" },
  { name: "Meteor", icon: "☄️", type: "dmg", mult: 3.0, cd: 14, color: "#ff3da5" },
];

export default function Game() {
  const [cur, setCur] = useState({ diamonds: 0, jade: 2_094_819, soul: 17_865_124 });
  const [hero, setHero] = useState({ class: 1, level: 55, exp: 0, soulPower: 59, strength: 60, agility: 24, intelligence: 88, vitality: 41, unspent: 6 });
  const [items, setItems] = useState(SEED_ITEMS);
  // equipped loadout: slot -> item id (one item per slot). Only equipped gear
  // feeds combat stats, so choosing a loadout matters.
  const [equipped, setEquipped] = useState(() => {
    const m = {};
    for (const it of SEED_ITEMS) if (!(it.slot in m)) m[it.slot] = it.id;
    return m;
  });
  const [sel, setSel] = useState(null);
  const [tab, setTab] = useState("bag");
  const [acc, setAcc] = useState({ exp: 0, soul: 0, jade: 0 });
  const [toast, setToast] = useState(null);
  const [showClass, setShowClass] = useState(false);
  const [market, setMarket] = useState(MARKET_SEED); // NPC gear for sale
  const [listings, setListings] = useState([]);      // player's active listings [{item, price}]
  const [listing, setListing] = useState(null);      // item being priced in the list modal
  const [pop, setPop] = useState(null); // floating combat text
  const idRef = useRef(1000);
  const [combat, setCombat] = useState({ stage: 1, heroHp: null, enemy: enemyForStage(1), kills: 0, downed: false, cds: SKILLS.map((s) => s.cd) });
  const tRef = useRef();
  const derivedRef = useRef(null);

  const flash = (txt, color = C.ink) => { setToast({ txt, color }); clearTimeout(tRef.current); tRef.current = setTimeout(() => setToast(null), 1800); };

  const isEquipped = (it) => equipped[it.slot] === it.id;
  const toggleEquip = (it) => setEquipped((e) => {
    if (e[it.slot] === it.id) { const n = { ...e }; delete n[it.slot]; return n; } // unequip
    return { ...e, [it.slot]: it.id }; // equip (replaces whatever was in the slot)
  });

  // derived combat panel from the EQUIPPED loadout only
  const derived = (() => {
    let aMin = hero.strength * 2, aMax = hero.strength * 4, def = Math.round(hero.vitality * 0.4), crit = 500;
    let hp = hero.vitality * 20 + hero.level * 12, mp = hero.intelligence * 15 + hero.level * 8;
    for (const it of items) { if (!isEquipped(it)) continue; for (const [k, b] of Object.entries(it.base)) {
      const v = b + bonusOf(b, it.plus, it.stars);
      if (k === "atk_min") aMin += v; else if (k === "atk_max") aMax += v;
      else if (k === "defence") def += v; else if (k === "hp") hp += v; else if (k === "mp") mp += v;
      else if (k === "crit_bps") crit += v; else if (k === "max_attack") aMax += v;
    } }
    return { aMin, aMax, def, crit, hp, mp };
  })();
  derivedRef.current = derived; // latest combat stats for the battle loop

  // auto-battler: runs idle. Hero auto-attacks with derived stats; kills drop
  // Soul + Exp into `acc`, which Claim banks into grant_xp/level-ups.
  useEffect(() => {
    const id = setInterval(() => {
      const d = derivedRef.current;
      if (!d) return;
      setCombat((cb) => {
        let { stage, heroHp, enemy, kills, cds } = cb;
        const heroMax = d.hp;
        if (heroHp == null) heroHp = heroMax;
        if (!enemy) enemy = enemyForStage(stage);
        cds = cds && cds.length === SKILLS.length ? cds.map((c) => c - 1) : SKILLS.map((s) => s.cd);
        // basic attack
        const crit = Math.random() * 10000 < d.crit;
        let dmg = Math.round(d.aMin + Math.random() * Math.max(1, d.aMax - d.aMin));
        if (crit) dmg = Math.round(dmg * 1.8);
        let popTxt = crit ? `CRIT ${dmg}` : `${dmg}`, popKind = crit ? "crit" : "hit";
        // cast the first ready skill
        let heal = 0;
        const ready = cds.findIndex((c) => c <= 0);
        if (ready >= 0) {
          const sk = SKILLS[ready];
          cds = cds.slice(); cds[ready] = sk.cd;
          if (sk.type === "heal") { heal = Math.round(heroMax * sk.amt); popTxt = `${sk.icon} ${sk.name} +${heal}`; }
          else { const sd = Math.round(d.aMax * sk.mult); dmg += sd; popTxt = `${sk.icon} ${sk.name} ${sd}`; }
          popKind = "skill";
        }
        setPop({ id: Math.random(), txt: popTxt, crit: popKind === "crit", skill: popKind === "skill" });
        heroHp = Math.min(heroMax, heroHp + heal);
        const eHp = enemy.hp - dmg;
        if (eHp <= 0) {
          const soulDrop = Math.round(enemy.hpMax * 0.015) + stage;
          const expDrop = Math.round(stage * 6 + enemy.hpMax * 0.02);
          const jadeDrop = enemy.boss ? stage * 3 : Math.random() < 0.25 ? stage : 0;
          setAcc((a) => ({ exp: a.exp + expDrop, soul: a.soul + soulDrop, jade: a.jade + jadeDrop }));
          const next = stage + 1;
          return { stage: next, heroHp: Math.min(heroMax, heroHp + Math.round(heroMax * 0.25)), enemy: enemyForStage(next), kills: kills + 1, downed: false, cds };
        }
        // enemy strikes back, mitigated by defence
        const eDmg = Math.max(1, Math.round(enemy.atk * (1 - d.def / (d.def + 240))));
        const hHp = heroHp - eDmg;
        if (hHp <= 0) {
          const back = Math.max(1, stage - 1);
          return { stage: back, heroHp: heroMax, enemy: enemyForStage(back), kills, downed: true, cds };
        }
        return { ...cb, heroHp: hHp, enemy: { ...enemy, hp: eHp }, downed: false, cds };
      });
    }, 650);
    return () => clearInterval(id);
  }, []);

  const update = (id, patch) => setItems((xs) => xs.map((it) => (it.id === id ? { ...it, ...patch } : it)));

  function doEnhance(it) {
    const target = it.plus + 1;
    const cap = guaranteedCap(it.slot, it.q);
    const cost = enhanceCost(target);
    if (cur.jade < cost) return flash("Not enough Jade", C.danger);
    setCur((c) => ({ ...c, jade: c.jade - cost }));
    if (target <= cap) { // guaranteed
      update(it.id, { plus: target }); setSel({ ...it, plus: target });
      flash(`Enhanced to +${target}`, C.jade);
    } else { // chance
      const over = target - cap;
      const ok = Math.random() * 10000 < successBps(over);
      if (ok) { update(it.id, { plus: target }); setSel({ ...it, plus: target }); flash(`Success! +${target}`, C.jade); }
      else flash(`Failed — Jade burned`, C.danger);
    }
  }
  function doAwaken(it) {
    if (it.stars >= 10) return flash("Max stars", C.sub);
    const cost = starCost(it.stars + 1);
    if (cur.soul < cost) return flash("Not enough Soul", C.danger);
    setCur((c) => ({ ...c, soul: c.soul - cost }));
    update(it.id, { stars: it.stars + 1 }); setSel({ ...it, stars: it.stars + 1 });
    flash(`Awakened ★ ${it.stars + 1}`, C.magenta);
  }
  function claim() {
    setCur((c) => ({ ...c, soul: c.soul + acc.soul, jade: c.jade + acc.jade }));
    setHero((h) => {
      let { level, exp, strength, agility, intelligence, vitality, unspent } = { ...h };
      exp += acc.exp; let gained = 0;
      while (exp >= xpForLevel(level) && level < 60) { exp -= xpForLevel(level); level++; strength += 2; agility += 2; intelligence += 2; vitality += 2; unspent += 3; gained++; }
      if (gained) flash(`Level up! → ${level}`, C.gold);
      return { ...h, level, exp, strength, agility, intelligence, vitality, unspent, soulPower: h.soulPower + gained };
    });
    setAcc({ exp: 0, soul: 0, jade: 0 });
  }
  function allocate(stat) {
    if (hero.unspent <= 0) return;
    setHero((h) => ({ ...h, unspent: h.unspent - 1, [stat]: h[stat] + 1 }));
  }
  function buy(m) { // maps to marketplace.buy (Jade -> seller/treasury/burn, royalty to creator)
    if (cur.jade < m.price) return flash("Not enough Jade", C.danger);
    setCur((c) => ({ ...c, jade: c.jade - m.price }));
    const { price, ...gear } = m;
    const nid = ++idRef.current;
    setItems((xs) => [...xs, { ...gear, id: nid }]);
    setMarket((ms) => ms.filter((x) => x.id !== m.id));
    flash(`Bought ${m.name}`, C.jade);
  }
  function confirmList(item, price) { // maps to marketplace.list (escrow)
    if (equipped[item.slot] === item.id) setEquipped((e) => { const n = { ...e }; delete n[item.slot]; return n; });
    setItems((xs) => xs.filter((i) => i.id !== item.id));
    setListings((ls) => [...ls, { item, price }]);
    setListing(null);
    flash(`Listed ${item.name} for ${num(price)} Jade`, C.jade);
  }
  function delist(entry) { // maps to marketplace.cancel (escrow -> owner)
    setListings((ls) => ls.filter((l) => l !== entry));
    setItems((xs) => [...xs, entry.item]);
    flash(`Delisted ${entry.item.name}`, C.sub);
  }
  function mintHero(cls) {
    // maps to assets.mint_hero (FREE) with the chosen class -> class starting stats
    setHero({ class: cls.id, level: 1, exp: 0, soulPower: 1, strength: cls.start[0], agility: cls.start[1], intelligence: cls.start[2], vitality: cls.start[3], unspent: 0 });
    setShowClass(false);
    flash(`Minted ${cls.name}!`, C.jade);
  }

  const num = (n) => n.toLocaleString();

  return (
    <div style={{ ...round, background: `linear-gradient(160deg, ${C.frost}, ${C.bg})`, color: C.ink, minHeight: "100%" }} className="w-full">
      <style>{`@import url('https://fonts.googleapis.com/css2?family=Pixelify+Sans:wght@400;500;600;700&family=Nunito:wght@600;700;800;900&display=swap');`}</style>

      <div className="mx-auto" style={{ maxWidth: 460, paddingBottom: 90 }}>
        {/* top bar */}
        <div className="flex items-center justify-between px-3 py-3">
          <Wallet icon={<Gem size={16} color={C.gem} />} v={num(cur.diamonds)} />
          <Wallet icon={<Sparkles size={16} color={C.jade} />} v={num(cur.jade)} c={C.jade} />
          <Wallet icon={<Droplet size={16} color={C.soul} />} v={num(cur.soul)} c={C.soul} />
        </div>

        {/* hero card */}
        <div className="mx-3 p-4 rounded-3xl" style={{ background: C.panel, boxShadow: "0 10px 24px rgba(60,50,90,.12)" }}>
          <div className="flex items-center gap-3">
            <div className="rounded-2xl flex items-center justify-center" style={{ width: 76, height: 76, background: `radial-gradient(circle at 50% 35%, #6d5cff55, ${C.frost})`, fontSize: 40 }}>🧙‍♀️</div>
            <div className="flex-1">
              <div className="flex items-center gap-2">
                <div style={{ ...px, fontSize: 18 }}>potymeat</div>
                <button onClick={() => setShowClass(true)} style={{ ...px, fontSize: 10, color: C.gem, background: "#f4f2fb", borderRadius: 10, padding: "2px 8px" }}>{CLASSES[hero.class].icon} {CLASSES[hero.class].name}</button>
              </div>
              <div style={{ color: C.sub, fontSize: 13 }}>Level {hero.level} · SoulPower {hero.soulPower}</div>
              <div className="mt-2 h-2 rounded-full" style={{ background: C.frost }}>
                <div className="h-2 rounded-full" style={{ width: `${Math.min(100, (hero.exp / xpForLevel(hero.level)) * 100)}%`, background: `linear-gradient(90deg, ${C.gold}, #ff8c1a)` }} />
              </div>
            </div>
          </div>
          <div className="grid grid-cols-3 gap-2 mt-3">
            <Stat icon={<Swords size={14} />} label="Attack" v={`${derived.aMin}-${derived.aMax}`} />
            <Stat icon={<Heart size={14} color={C.danger} />} label="HP" v={num(derived.hp)} />
            <Stat icon={<FlaskConical size={14} color={C.soul} />} label="MP" v={num(derived.mp)} />
            <Stat icon={<Shield size={14} />} label="Defence" v={derived.def} />
            <Stat icon={<Sparkles size={14} color={C.gold} />} label="Critical" v={`${(derived.crit / 100).toFixed(0)}%`} />
            <Stat icon={<Star size={14} color={C.magenta} />} label="Points" v={hero.unspent} highlight={hero.unspent > 0} />
          </div>
          {hero.unspent > 0 && (
            <div className="flex gap-2 mt-2">
              {["strength", "agility", "intelligence", "vitality"].map((s) => (
                <button key={s} onClick={() => allocate(s)} className="flex-1 py-1 rounded-xl" style={{ ...px, fontSize: 11, background: C.frost, color: C.ink }}>+{s.slice(0, 3).toUpperCase()}</button>
              ))}
            </div>
          )}
        </div>

        {/* tabs */}
        <div className="flex gap-2 px-3 mt-4">
          {[["bag", "Bag", Swords], ["battle", "Battle", Zap], ["market", "Trade", Gem]].map(([id, label, Icon]) => (
            <button key={id} onClick={() => setTab(id)} className="flex-1 py-2 rounded-2xl flex items-center justify-center gap-1"
              style={{ ...px, fontSize: 13, background: tab === id ? C.ink : C.panel, color: tab === id ? "#fff" : C.sub, boxShadow: "0 4px 12px rgba(60,50,90,.1)" }}>
              <Icon size={14} /> {label}
            </button>
          ))}
        </div>

        {/* content */}
        <div className="px-3 mt-3">
          {tab === "bag" && (
            <>
              <div style={{ fontSize: 11, color: C.sub, margin: "0 2px 6px" }}>Tap an item to inspect · <span style={{ color: C.jade }}>green dot</span> = equipped. Only equipped gear counts in combat.</div>
              <div className="grid grid-cols-4 gap-2">
                {items.map((it) => <Tile key={it.id} it={it} eq={isEquipped(it)} onClick={() => setSel(it)} />)}
              </div>
            </>
          )}
          {tab === "battle" && (
            <div className="rounded-3xl p-4" style={{ background: `linear-gradient(165deg, #bfe0ff, #8ec5ff)`, position: "relative", overflow: "hidden", minHeight: 220 }}>
              <div style={{ position: "absolute", inset: 0, opacity: .15, background: "repeating-linear-gradient(135deg,#fff 0 8px,transparent 8px 16px)" }} />
              <div className="flex items-center justify-between" style={{ position: "relative" }}>
                <span style={{ ...px, fontSize: 12, color: combat.enemy?.boss ? C.danger : C.ink }}>Stage {combat.stage}{combat.enemy?.boss ? " · BOSS" : ""}</span>
                <span style={{ ...px, fontSize: 12, color: C.ink }}>Kills {combat.kills}</span>
              </div>
              {/* enemy */}
              <div className="flex flex-col items-center" style={{ position: "relative", marginTop: 4 }}>
                <div style={{ fontSize: 46, filter: "drop-shadow(0 3px 4px #0003)" }}>{combat.enemy?.icon}</div>
                {pop && <span key={pop.id} style={{ ...px, position: "absolute", top: 0, color: pop.crit ? C.gold : pop.skill ? "#ffe08a" : "#fff", fontSize: pop.crit || pop.skill ? 16 : 13, fontWeight: 700, textShadow: "0 1px 3px #0008", animation: "rise .65s ease-out" }}>{pop.txt}</span>}
                <div style={{ ...px, fontSize: 12, color: combat.enemy?.boss ? C.danger : C.ink, marginTop: 2 }}>{combat.enemy?.name}</div>
                <div style={{ width: 190 }}><Bar frac={combat.enemy ? combat.enemy.hp / combat.enemy.hpMax : 1} color={C.danger} /></div>
              </div>
              {/* skill bar — auto-casts on cooldown */}
              <div className="flex gap-2 justify-center mt-3">
                {SKILLS.map((sk, i) => {
                  const rem = combat.cds ? combat.cds[i] : sk.cd;
                  const ready = rem <= 0;
                  return (
                    <div key={i} className="rounded-2xl flex items-center justify-center" style={{ width: 46, height: 46, position: "relative", overflow: "hidden", background: "#ffffffcc", border: ready ? `2px solid ${sk.color}` : "2px solid #ffffff00", boxShadow: ready ? `0 0 10px ${sk.color}aa` : "none" }}>
                      <span style={{ fontSize: 22, filter: ready ? "none" : "grayscale(.5)", opacity: ready ? 1 : .55 }}>{sk.icon}</span>
                      {!ready && <div style={{ position: "absolute", bottom: 0, left: 0, right: 0, height: `${(rem / sk.cd) * 100}%`, background: "#3a355022" }} />}
                    </div>
                  );
                })}
              </div>
              {/* hero HP */}
              <div className="rounded-2xl p-2 mt-3" style={{ background: "#ffffffcc", position: "relative" }}>
                <div className="flex items-center justify-between" style={{ fontSize: 11, color: C.sub }}>
                  <span>🧙‍♀️ Your HP</span>
                  <span style={{ ...px }}>{Math.max(0, Math.round(combat.heroHp ?? derived.hp)).toLocaleString()} / {derived.hp.toLocaleString()}</span>
                </div>
                <Bar frac={(combat.heroHp ?? derived.hp) / derived.hp} color={C.jade} />
                {combat.downed && <div style={{ ...px, fontSize: 11, color: C.danger, textAlign: "center", marginTop: 4 }}>Defeated — retreated to stage {combat.stage}. Enhance your gear!</div>}
              </div>
              {/* drops + claim */}
              <div className="rounded-2xl p-3 mt-2" style={{ background: "#ffffffcc" }}>
                <div className="grid grid-cols-3 gap-2 text-center">
                  <Drop icon="✨" label="Exp" v={acc.exp} c={C.gold} />
                  <Drop icon="💧" label="Soul" v={acc.soul} c={C.soul} />
                  <Drop icon="💚" label="Jade" v={acc.jade} c={C.jade} />
                </div>
                <button onClick={claim} className="w-full mt-3 py-3 rounded-2xl" style={{ ...px, fontSize: 15, color: "#fff", background: `linear-gradient(90deg, ${C.jade}, #1faa46)`, boxShadow: "0 6px 14px #1faa4655" }}>Claim Rewards</button>
              </div>
              <style>{`@keyframes rise{from{transform:translateY(8px);opacity:0}30%{opacity:1}to{transform:translateY(-26px);opacity:0}}`}</style>
            </div>
          )}
          {tab === "market" && (
            <div className="space-y-3">
              {/* Buy */}
              <div>
                <div style={{ ...px, fontSize: 12, color: C.ink, margin: "0 2px 6px" }}>Buy gear · pay in Jade</div>
                <div className="space-y-2">
                  {market.length === 0 && <div style={{ fontSize: 11, color: C.sub, textAlign: "center", padding: 8 }}>Sold out — restocks after more battles</div>}
                  {market.map((m) => (
                    <Row key={m.id} it={m}>
                      <button onClick={() => buy(m)} className="px-3 py-2 rounded-xl text-center" style={{ ...px, fontSize: 12, color: "#fff", background: cur.jade >= m.price ? C.jade : "#c3c0d0" }}>
                        Buy<div style={{ fontSize: 10, opacity: .9 }}>{num(m.price)}</div>
                      </button>
                    </Row>
                  ))}
                </div>
              </div>
              {/* Your listings */}
              {listings.length > 0 && (
                <div>
                  <div style={{ ...px, fontSize: 12, color: C.ink, margin: "0 2px 6px" }}>Your listings</div>
                  <div className="space-y-2">
                    {listings.map((l, i) => (
                      <Row key={i} it={l.item} sub={`Listed · ${num(l.price)} Jade · you get ${num(splitOf(l.price).seller)}`}>
                        <button onClick={() => delist(l)} className="px-3 py-2 rounded-xl" style={{ ...px, fontSize: 12, color: C.ink, background: C.frost }}>Delist</button>
                      </Row>
                    ))}
                  </div>
                </div>
              )}
              {/* Sell */}
              <div>
                <div style={{ ...px, fontSize: 12, color: C.ink, margin: "0 2px 6px" }}>Sell your gear</div>
                <div className="space-y-2">
                  {items.length === 0 && <div style={{ fontSize: 11, color: C.sub, textAlign: "center", padding: 8 }}>Nothing to sell</div>}
                  {items.map((it) => (
                    <Row key={it.id} it={it}>
                      <button onClick={() => setListing(it)} className="px-3 py-2 rounded-xl" style={{ ...px, fontSize: 12, color: "#fff", background: C.ink }}>List</button>
                    </Row>
                  ))}
                </div>
              </div>
              <div style={{ fontSize: 11, color: C.sub, textAlign: "center" }}>Split on every sale: seller 90% · 5% creator royalty · 5% fee (30% burned)</div>
            </div>
          )}
        </div>
      </div>

      {/* item inspect — the signature card */}
      {showClass && (
        <div onClick={() => setShowClass(false)} style={{ position: "fixed", inset: 0, background: "#1d153066", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 45, padding: 16 }}>
          <div onClick={(e) => e.stopPropagation()} className="rounded-3xl p-4" style={{ background: "#fff", width: "100%", maxWidth: 380 }}>
            <div style={{ ...px, fontSize: 16, textAlign: "center" }}>Mint a Hero</div>
            <div style={{ fontSize: 11, color: C.sub, textAlign: "center", marginBottom: 12 }}>Free mint · choose your class</div>
            <div className="grid grid-cols-2 gap-2">
              {CLASSES.map((c) => (
                <button key={c.id} onClick={() => mintHero(c)} className="rounded-2xl p-3 text-left" style={{ background: hero.class === c.id ? "#efe9ff" : "#f4f2fb", border: `2px solid ${hero.class === c.id ? C.gem : "transparent"}` }}>
                  <div style={{ fontSize: 24 }}>{c.icon}</div>
                  <div style={{ ...px, fontSize: 13 }}>{c.name}</div>
                  <div style={{ fontSize: 10, color: C.sub }}>{c.blurb}</div>
                  <div style={{ fontSize: 10, color: C.ink, marginTop: 2 }}>STR {c.start[0]} · AGI {c.start[1]} · INT {c.start[2]} · VIT {c.start[3]}</div>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {sel && <Inspect it={sel} equipped={isEquipped(sel)} onToggleEquip={() => toggleEquip(sel)} onClose={() => setSel(null)} onEnhance={() => doEnhance(items.find((i) => i.id === sel.id))} onAwaken={() => doAwaken(items.find((i) => i.id === sel.id))} cur={cur} />}

      {listing && <ListModal item={listing} onClose={() => setListing(null)} onConfirm={confirmList} />}

      {toast && <div style={{ ...px, position: "fixed", left: "50%", bottom: 24, transform: "translateX(-50%)", background: "#fff", color: toast.color, padding: "10px 18px", borderRadius: 16, boxShadow: "0 8px 20px rgba(0,0,0,.18)", fontSize: 13, zIndex: 50 }}>{toast.txt}</div>}
    </div>
  );
}

const plusLabel = (it) => (it.stars > 1 ? `+${it.plus}|${it.stars}` : `+${it.plus}`);

function Wallet({ icon, v, c = "#3a3550" }) {
  return <div className="flex items-center gap-1 px-3 py-1 rounded-full" style={{ background: "#ffffffcc", boxShadow: "0 2px 8px rgba(60,50,90,.1)" }}>{icon}<span style={{ ...px, fontSize: 13, color: c }}>{v}</span></div>;
}
function Stat({ icon, label, v, highlight }) {
  return <div className="rounded-2xl px-2 py-2" style={{ background: highlight ? "#fff3d6" : "#f4f2fb" }}>
    <div className="flex items-center gap-1" style={{ color: "#8a85a3", fontSize: 11 }}>{icon}{label}</div>
    <div style={{ ...px, fontSize: 14, marginTop: 2 }}>{v}</div>
  </div>;
}
function Drop({ icon, label, v, c }) {
  return <div><div style={{ fontSize: 18 }}>{icon}</div><div style={{ fontSize: 10, color: "#8a85a3" }}>{label}</div><div style={{ ...px, fontSize: 14, color: c }}>{v.toLocaleString()}</div></div>;
}
function Bar({ frac, color }) {
  return (
    <div className="rounded-full mt-1" style={{ height: 7, width: "100%", background: "#0000001f", overflow: "hidden" }}>
      <div style={{ height: 7, width: `${Math.max(0, Math.min(1, frac)) * 100}%`, background: color, borderRadius: 9999, transition: "width .25s linear" }} />
    </div>
  );
}
function Row({ it, sub, children }) {
  return (
    <div className="flex items-center gap-3 p-2 rounded-2xl" style={{ background: C.panel }}>
      <div className="rounded-xl flex items-center justify-center" style={{ width: 44, height: 44, background: C.frost, fontSize: 22 }}>{it.icon}</div>
      <div className="flex-1 min-w-0">
        <div style={{ ...px, fontSize: 13, color: QCOLOR[it.q] }}>{it.name} {plusLabel(it)}</div>
        <div style={{ fontSize: 11, color: C.sub, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{sub ?? `${QUALITY[it.q]} · Lv.${it.lvl}`}</div>
      </div>
      {children}
    </div>
  );
}
function ListModal({ item, onClose, onConfirm }) {
  const [price, setPrice] = useState(1200);
  const s = splitOf(price);
  const step = (d) => setPrice((p) => Math.max(100, p + d));
  const SplitRow = ({ label, v, c, strong }) => (
    <div className="flex items-center justify-between" style={{ padding: "3px 0" }}>
      <span style={{ fontSize: 12, color: C.sub }}>{label}</span>
      <span style={{ ...px, fontSize: strong ? 15 : 13, color: c }}>{v.toLocaleString()}</span>
    </div>
  );
  const Step = ({ d, children }) => (
    <button onClick={() => step(d)} className="rounded-xl" style={{ ...px, fontSize: 12, padding: "6px 8px", background: C.frost, color: C.ink }}>{children}</button>
  );
  return (
    <div onClick={onClose} style={{ position: "fixed", inset: 0, background: "#1d153066", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 46, padding: 16 }}>
      <div onClick={(e) => e.stopPropagation()} className="rounded-3xl p-4" style={{ background: "#fff", width: "100%", maxWidth: 380 }}>
        <div style={{ ...px, fontSize: 16, textAlign: "center" }}>List {item.name}</div>
        <div style={{ fontSize: 11, color: C.sub, textAlign: "center", marginBottom: 12 }}>Set your asking price (Jade)</div>
        <div className="flex items-center justify-center gap-2 mb-3">
          <Step d={-500}>−500</Step><Step d={-100}>−100</Step>
          <div style={{ ...px, fontSize: 20, color: C.jade, minWidth: 92, textAlign: "center" }}>{price.toLocaleString()}</div>
          <Step d={100}>+100</Step><Step d={500}>+500</Step>
        </div>
        <div className="rounded-2xl p-3" style={{ background: "#f4f2fb" }}>
          <SplitRow label="You receive" v={s.seller} c={C.jade} strong />
          <SplitRow label="Creator royalty (5%)" v={s.royalty} c={C.gem} />
          <SplitRow label="Treasury fee" v={s.treasury} c={C.ink} />
          <SplitRow label="Burned 🔥 (deflationary)" v={s.burn} c={C.danger} />
        </div>
        <button onClick={() => onConfirm(item, price)} className="w-full py-3 mt-3 rounded-2xl" style={{ ...px, fontSize: 14, color: "#fff", background: `linear-gradient(90deg,${C.jade},#1faa46)` }}>Confirm listing</button>
      </div>
    </div>
  );
}
function Tile({ it, onClick, eq }) {
  return (
    <button onClick={onClick} className="rounded-2xl flex flex-col items-center justify-center relative" style={{ aspectRatio: "1", background: C.panel, boxShadow: "0 3px 10px rgba(60,50,90,.08)", border: `2px solid ${eq ? C.jade : QCOLOR[it.q] + "33"}` }}>
      <span style={{ fontSize: 28 }}>{it.icon}</span>
      <span style={{ ...px, position: "absolute", right: 6, bottom: 4, fontSize: 11, color: it.q >= 3 ? C.magenta : C.ink }}>{plusLabel(it)}</span>
      <span style={{ position: "absolute", left: 6, top: 6, width: 8, height: 8, borderRadius: 8, background: QCOLOR[it.q] }} />
      {eq && <span style={{ position: "absolute", right: 5, top: 5, width: 9, height: 9, borderRadius: 9, background: C.jade, border: "1.5px solid #fff" }} />}
    </button>
  );
}

function Inspect({ it, onClose, onEnhance, onAwaken, equipped, onToggleEquip }) {
  const cap = guaranteedCap(it.slot, it.q);
  const target = it.plus + 1;
  const guaranteed = target <= cap;
  const over = target - cap;
  const stats = Object.entries(it.base).map(([k, b]) => {
    const bonus = bonusOf(b, it.plus, it.stars);
    return { k, label: affixLabel[k], base: b, bonus, total: b + bonus };
  });
  return (
    <div onClick={onClose} style={{ position: "fixed", inset: 0, background: "#1d153066", display: "flex", alignItems: "center", justifyContent: "center", zIndex: 40, padding: 16 }}>
      <div onClick={(e) => e.stopPropagation()} className="rounded-3xl overflow-hidden" style={{ width: "100%", maxWidth: 380, background: "#fff" }}>
        {/* gradient art panel */}
        <div style={{ background: `radial-gradient(circle at 50% 40%, ${C.cardA}, ${C.cardB})`, padding: "18px 16px", position: "relative" }}>
          <div style={{ ...px, color: "#cdb6ff", fontSize: 13 }}>Lv.{it.lvl}</div>
          <div style={{ ...px, textAlign: "center", color: C.magenta, fontSize: 18, marginTop: 4 }}>{it.name} {plusLabel(it)}</div>
          <div style={{ textAlign: "center", fontSize: 60, margin: "8px 0" }}>{it.icon}</div>
          <div className="flex justify-center gap-1">
            {Array.from({ length: 10 }).map((_, i) => <Star key={i} size={14} color={i < it.stars ? C.magenta : "#ffffff44"} fill={i < it.stars ? C.magenta : "transparent"} />)}
          </div>
        </div>
        {/* stats */}
        <div className="p-4">
          <div className="space-y-2">
            {stats.map((s) => (
              <div key={s.k} className="flex items-center justify-between px-3 py-2 rounded-xl" style={{ background: "#f4f2fb" }}>
                <span style={{ color: C.sub, fontSize: 13 }}>{s.label}</span>
                <span style={{ ...px, fontSize: 14 }}>{fmtVal(s.k, s.total)} <span style={{ color: "#36a0ff", fontSize: 11 }}>({fmtVal(s.k, s.base)}{s.bonus ? `+${fmtVal(s.k, s.bonus)}` : ""})</span></span>
              </div>
            ))}
          </div>
          <button onClick={onToggleEquip} className="w-full py-2 mt-3 rounded-2xl" style={{ ...px, fontSize: 13, color: equipped ? C.jade : "#fff", background: equipped ? "#eafaf0" : C.ink, border: equipped ? `2px solid ${C.jade}` : "none" }}>
            {equipped ? `✓ Equipped (${it.slot}) — tap to unequip` : `Equip to ${it.slot} slot`}
          </button>
          <div className="flex gap-2 mt-3">
            <button onClick={onEnhance} className="flex-1 py-3 rounded-2xl" style={{ ...px, fontSize: 13, color: "#fff", background: guaranteed ? `linear-gradient(90deg,${C.jade},#1faa46)` : `linear-gradient(90deg,${C.gold},#ff8c1a)` }}>
              {it.plus >= 9 ? "Maxed" : guaranteed ? `Enhance → +${target}` : `Try +${target} (${(successBps(over) / 100).toFixed(0)}%)`}
              <div style={{ fontSize: 10, opacity: .85 }}>{enhanceCost(target)} Jade</div>
            </button>
            <button onClick={onAwaken} className="flex-1 py-3 rounded-2xl" style={{ ...px, fontSize: 13, color: "#fff", background: it.stars >= 10 ? "#bbb" : `linear-gradient(90deg,${C.magenta},#c81d83)` }}>
              {it.stars >= 10 ? "★ Max" : `Awaken ★ ${it.stars + 1}`}
              <div style={{ fontSize: 10, opacity: .85 }}>{starCost(it.stars + 1)} Soul</div>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
