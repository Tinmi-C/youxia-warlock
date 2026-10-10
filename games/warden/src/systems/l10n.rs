//! Localization (UI7): every player-facing string lives here as a `Bi(zh, en)`
//! constant and is picked by `Lang`. Dynamic strings are assembled at call
//! sites from these fragments (format templates keep their argument order in
//! both languages). Code identifiers and logs stay English (team rule).
//! Toggle: `L` key or the top-right button; persisted to `ui_prefs.txt`.

use bevy::prelude::*;

use crate::components::AttackType;
use crate::resources::{FusionDef, Lang, TowerDef, UiPrefsPath};

/// A bilingual (Chinese, English) constant.
pub struct Bi(pub &'static str, pub &'static str);

impl Bi {
    pub fn pick(&self, lang: Lang) -> &'static str {
        match lang {
            Lang::Zh => self.0,
            Lang::En => self.1,
        }
    }
}

fn pick2<'a>(zh: &'a str, en: &'a str, lang: Lang) -> &'a str {
    match lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

pub fn tower_label(def: &TowerDef, lang: Lang) -> &'static str {
    pick2(def.label, def.label_en, lang)
}

pub fn fusion_label(def: &FusionDef, lang: Lang) -> &'static str {
    pick2(def.label, def.label_en, lang)
}

pub fn enemy_label(def: &crate::resources::EnemyDef, lang: Lang) -> &'static str {
    pick2(def.label, def.label_en, lang)
}

/// The between-waves "next wave" preview (UI8): composition + reward, so the
/// slow-phase economy/placement decisions are informed (requirements §3/§10).
pub fn next_wave_preview(lang: Lang, composition: &str, reward: u32) -> String {
    match lang {
        Lang::Zh => format!("下一波：{} · 奖励{}金", composition, reward),
        Lang::En => format!("Next wave: {} · reward {}g", composition, reward),
    }
}

// --- Static copy ------------------------------------------------------------

pub const HELP_LINE: Bi = Bi(
    "点底部卡片选塔 → 点绿格放置 · 右键取消 · 点两座已放塔尝试融合 · 右下按钮或空格 开下一波",
    "Click a bottom card to pick a tower → click a green slot to place · Right-click cancels · Click two towers to fuse · Button/Space starts the next wave",
);

pub const START_WAVE: Bi = Bi("▶ 开始下一波", "▶ Next Wave");

/// The toggle button shows the *other* language's name (what you switch to).
pub fn lang_button_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Zh => "EN",
        Lang::En => "中文",
    }
}

pub const PHASE_BUILD: Bi = Bi("建造期", "Build");
pub const PHASE_COMBAT: Bi = Bi("战斗期", "Combat");

// HUD status-line key labels (UI4/UI7); the separators stay in the template.
pub const GOLD_EQ: Bi = Bi("金币=", "Gold=");
pub const BASE_EQ: Bi = Bi("基地=", "Base=");
pub const SEL: Bi = Bi("选中:", "Sel:");
pub const HAND: Bi = Bi("手牌", "Hand");
pub const COINS_EQ: Bi = Bi("仓库币=", "Coins=");
pub const CHOICE: Bi = Bi("三选一：", "Choice: ");

pub const END_BANNER: Bi = Bi("🏁 本局结束", "🏁 Run Over");
pub const END_HINT: Bi = Bi(
    "按 1/2 购买 Meta 升级，或点「再来一局」",
    "Press 1/2 to buy meta upgrades, or click Retry",
);
pub const RETRY: Bi = Bi("再来一局", "Retry");

// --- Command card (UI9, requirements §16) ------------------------------------

pub const INFO_NONE: Bi = Bi(
    "左键选择塔或空塔位 · 点空白处取消",
    "Left-click a tower or a slot · click blank to cancel",
);
pub const INFO_SLOT: Bi = Bi("空塔位 · 从命令卡选塔建造", "Empty slot · pick a tower from the card");
pub const SELL: Bi = Bi("卖出", "Sell");
pub const FUSE: Bi = Bi("融合", "Fuse");
pub const RESERVED: Bi = Bi("预留", "Reserved");
pub const SPEED_2X: Bi = Bi("2x", "2x");
pub const PAUSE: Bi = Bi("暂停", "Pause");
pub const FUSE_TARGET_HINT: Bi = Bi(
    "选择另一座塔进行融合 · Esc 取消",
    "Pick another tower to fuse · Esc cancels",
);
pub const FUSE_CANCEL: Bi = Bi("取消", "Cancel");
pub const FUSE_NONE: Bi = Bi("无可配对塔", "No pairable tower");

// --- Tooltip (UI12, requirements §16.8) ---------------------------------------

pub const TT_HOTKEY: Bi = Bi("热键", "Hotkey");
pub const TT_COST: Bi = Bi("造价", "Cost");
pub const TT_SELL_FOR: Bi = Bi("卖出回收", "Sells for");
pub const TT_FUSE_DESC: Bi = Bi(
    "与另一座塔融合成高级塔（手续费=原料造价 20%）",
    "Fuse with another tower (fee = 20% of ingredients)",
);
pub const TT_RESERVED: Bi = Bi("预留格（后续升级/技能）", "Reserved (upgrades / skills later)");

pub fn tower_role(def: &crate::resources::TowerDef, lang: Lang) -> &'static str {
    pick2(def.role, def.role_en, lang)
}

/// Fusion recipe preview line for the info zone (UI11, §16.6).
pub fn fusion_preview(lang: Lang, recipe: &str, fee: u32) -> String {
    match lang {
        Lang::Zh => format!("配方：{} · 手续费{}金", recipe, fee),
        Lang::En => format!("Recipe: {} · fee {}g", recipe, fee),
    }
}

pub const ATTACK_PHYSICAL: Bi = Bi("物理", "Physical");
pub const ATTACK_MAGIC: Bi = Bi("魔法", "Magic");
pub const ATTACK_MIXED: Bi = Bi("混合·无视护甲", "Mixed (ignores armor)");

pub const FUSE_HINT: Bi = Bi(
    "点另一座塔=尝试融合 · 右键=取消",
    "Click another tower to fuse · Right-click to cancel",
);

pub const BASE_TOWER_FALLBACK: Bi = Bi("基础塔", "Base tower");
pub const FUSED_TOWER_FALLBACK: Bi = Bi("融合塔", "Fused tower");

// --- Skills (SK1, requirements §17) --------------------------------------------

/// Targeting hint while a point skill awaits its ground confirm (§17.3).
pub const SKILL_TARGET_HINT: Bi = Bi(
    "选择目标点 · 左键确认 · Esc/右键取消",
    "Pick a target point · Left-click confirms · Esc/Right-click cancels",
);

/// Skill display name (§17.2).
pub fn skill_name(kind: crate::resources::SkillId, lang: Lang) -> &'static str {
    use crate::resources::SkillId;
    pick2(
        match kind {
            SkillId::MeteorRain => "流星火雨",
            SkillId::FrostNova => "冰霜新星",
            SkillId::WarFrenzy => "战意狂暴",
        },
        match kind {
            SkillId::MeteorRain => "Meteor Rain",
            SkillId::FrostNova => "Frost Nova",
            SkillId::WarFrenzy => "War Frenzy",
        },
        lang,
    )
}

/// Tooltip body: effect numbers + cooldown — every number from the
/// SkillBalance single source (SK11).
pub fn skill_tooltip(
    lang: Lang,
    kind: crate::resources::SkillId,
    balance: &crate::resources::SkillBalance,
) -> String {
    use crate::resources::SkillId;
    use crate::systems::skills::{SKILL_FROST_NOVA, SKILL_METEOR_RAIN};
    let cd = balance.cooldown(kind.def_id(), 1);
    let effect = match kind {
        SkillId::MeteorRain => {
            let (count, dmg, radius) = (
                balance.meteor_count(SKILL_METEOR_RAIN, 1),
                balance.meteor_damage(SKILL_METEOR_RAIN, 1),
                balance.radius(SKILL_METEOR_RAIN, 1),
            );
            match lang {
                Lang::Zh => format!("{count} 发流星依次落下，每发对半径 {radius:.0} 内造成 {dmg:.0} 魔法伤害（穿物理护甲）"),
                Lang::En => format!("{count} meteors fall in sequence, each dealing {dmg:.0} magic damage in radius {radius:.0} (ignores armor)"),
            }
        }
        SkillId::FrostNova => {
            let radius = balance.radius(SKILL_FROST_NOVA, 1);
            let factor = balance.nova_slow_factor;
            let duration = balance.nova_slow_duration;
            match lang {
                Lang::Zh => format!("点半径 {radius:.0} 内敌人减速 {:.0}%，持续 {duration:.0} 秒（与塔减速同存取更强者）", (1.0 - factor) * 100.0),
                Lang::En => format!("Slows enemies within radius {radius:.0} by {:.0}% for {duration:.0}s (strongest slow wins)", (1.0 - factor) * 100.0),
            }
        }
        SkillId::WarFrenzy => {
            let (mult, duration) = (balance.frenzy_speed_mult, balance.frenzy_duration);
            match lang {
                Lang::Zh => format!("{duration:.0} 秒内全场塔攻速 +{:.0}%（含持续中新建的塔）", (mult - 1.0) * 100.0),
                Lang::En => format!("All towers gain +{:.0}% attack speed for {duration:.0}s (towers built mid-frenzy too)", (mult - 1.0) * 100.0),
            }
        }
    };
    match lang {
        Lang::Zh => format!("{effect}\n冷却 {cd:.0} 秒 · 免费施放"),
        Lang::En => format!("{effect}\nCooldown {cd:.0}s · free to cast"),
    }
}

// --- SK10: tower-skill card strings ---------------------------------------------

/// SK10: the paid kit-upgrade button label.
pub const SKILL_UPGRADE: Bi = Bi("升级技能", "Upgrade");
/// SK10: the upgrade tooltip body.
pub const TT_SKILL_UPGRADE: Bi = Bi(
    "花费金币，该塔全部技能等级 +1（被动数值随之提升）",
    "Spend gold to raise this tower's whole skill kit one level",
);

/// SK10: display name of a registry skill (archer + fusion kits today).
/// `def` is the base string def id (WX2).
pub fn tower_skill_name(def: &str, lang: Lang) -> &'static str {
    use crate::systems::archer_skills::{SKILL_EAGLE_EYE, SKILL_FIRE_ARROWS, SKILL_MULTISHOT};
    use crate::systems::fusion_skills::{
        SKILL_ARCANE_BLAST, SKILL_CONCUSSION, SKILL_DEADEYE, SKILL_PIERCING_BOLT,
    };
    match def {
        SKILL_EAGLE_EYE => pick2("鹰眼", "Eagle Eye", lang),
        SKILL_MULTISHOT => pick2("多重射击", "Multishot", lang),
        SKILL_FIRE_ARROWS => pick2("火焰箭", "Fire Arrows", lang),
        SKILL_DEADEYE => pick2("狙杀", "Deadeye", lang),
        SKILL_ARCANE_BLAST => pick2("奥术爆轰", "Arcane Blast", lang),
        SKILL_PIERCING_BOLT => pick2("贯穿箭", "Piercing Bolt", lang),
        SKILL_CONCUSSION => pick2("震荡冲击", "Concussion", lang),
        _ => pick2("技能", "Skill", lang),
    }
}

/// SK10: tooltip body with the level's live numbers (SkillBalance values).
pub fn tower_skill_tooltip(def: &str, _level: u8, lang: Lang) -> String {
    use crate::systems::archer_skills::{SKILL_EAGLE_EYE, SKILL_FIRE_ARROWS, SKILL_MULTISHOT};
    use crate::systems::fusion_skills::{
        SKILL_ARCANE_BLAST, SKILL_CONCUSSION, SKILL_DEADEYE, SKILL_PIERCING_BOLT,
    };
    match def {
        SKILL_EAGLE_EYE => pick2(
            "被动：暴击率与暴击倍率提升（等级越高越强）",
            "Passive: raises crit chance and multiplier with level",
            lang,
        )
        .to_string(),
        SKILL_MULTISHOT => pick2(
            "朝射程内最近的数个目标各射一箭 · 冷却 25 秒",
            "One arrow at each of the nearest targets in range · 25s cooldown",
            lang,
        )
        .to_string(),
        SKILL_FIRE_ARROWS => pick2(
            "限时强化普攻伤害 · 冷却 20 秒 · 右键开关自动施放",
            "Timed normal-attack damage boost · 20s cooldown · right-click toggles autocast",
            lang,
        )
        .to_string(),
        SKILL_DEADEYE => pick2(
            "立即狙击射程内血量最高的敌人 · 伤害为普攻数倍 · 冷却 30 秒",
            "Snipes the highest-HP enemy in range for a multiple of attack damage · 30s cooldown",
            lang,
        )
        .to_string(),
        SKILL_ARCANE_BLAST => pick2(
            "对目标区域造成魔法伤害 · 冷却 35 秒",
            "Magic damage in an area · 35s cooldown",
            lang,
        )
        .to_string(),
        SKILL_PIERCING_BOLT => pick2(
            "直线穿透箭，命中路径上所有敌人（混合伤害穿护甲） · 冷却 25 秒",
            "A bolt piercing every enemy along its path (mixed damage) · 25s cooldown",
            lang,
        )
        .to_string(),
        SKILL_CONCUSSION => pick2(
            "目标区域伤害并减速 · 冷却 30 秒",
            "Damage and slow in an area · 30s cooldown",
            lang,
        )
        .to_string(),
        _ => String::new(),
    }
}

// --- Formatted copy (format! needs literals, so each helper matches on lang) --

pub fn hud_wave(lang: Lang, current: usize, total: usize, phase: &str) -> String {
    match lang {
        Lang::Zh => format!("第{}波/共{}（{}）", current, total, phase),
        Lang::En => format!("Wave {}/{} ({})", current, total, phase),
    }
}

pub fn hud_settle(lang: Lang, cost1: u32, cost2: u32) -> String {
    match lang {
        Lang::Zh => format!(
            "结算：按1 开局必含弓箭手塔（{}币） · 按2 开局+20金（{}币） · P 再来一局",
            cost1, cost2
        ),
        Lang::En => format!(
            "End: 1=archer hand ({}c) · 2=+20 start gold ({}c) · P=retry",
            cost1, cost2
        ),
    }
}

pub fn card_cost(lang: Lang, name: &str, cost: u32) -> String {
    match lang {
        Lang::Zh => format!("{} {}金", name, cost),
        Lang::En => format!("{} {}g", name, cost),
    }
}

pub fn offer_cost(lang: Lang, name: &str, cost: u32) -> String {
    match lang {
        Lang::Zh => format!("[商店] {} {}金", name, cost),
        Lang::En => format!("[Shop] {} {}g", name, cost),
    }
}

pub fn tower_stats(lang: Lang, damage: f32, speed: f32, range: f32) -> String {
    match lang {
        Lang::Zh => format!("伤害 {:.0} · 攻速 {:.1}/秒 · 射程 {:.1}", damage, speed, range),
        Lang::En => format!("Damage {:.0} · Atk {:.1}/s · Range {:.1}", damage, speed, range),
    }
}

pub fn upgrade1_label(lang: Lang, cost: u32) -> String {
    match lang {
        Lang::Zh => format!("升级1 开局必含弓箭手塔（{}币）", cost),
        Lang::En => format!("Upgrade1: archer in opening hand ({}c)", cost),
    }
}

pub fn upgrade2_label(lang: Lang, cost: u32) -> String {
    match lang {
        Lang::Zh => format!("升级2 开局+20金（{}币）", cost),
        Lang::En => format!("Upgrade2: +20 start gold ({}c)", cost),
    }
}

/// The "三选一" option labels (AC4); the tower name is substituted per lang.
pub fn choice_label(
    kind: &crate::resources::ChoiceKind,
    defs: &crate::resources::TowerDefs,
    lang: Lang,
) -> String {
    use crate::resources::{ChoiceKind, StatKind};
    let name = |t: usize| {
        defs.list
            .get(t)
            .map(|d| tower_label(d, lang))
            .unwrap_or("?")
    };
    match kind {
        ChoiceKind::StatBoost { tower_type, stat } => match stat {
            StatKind::Damage => format!("{} {}", name(*tower_type), pick2("伤害 +20%", "damage +20%", lang)),
            StatKind::AttackSpeed => format!("{} {}", name(*tower_type), pick2("攻速 +20%", "atk speed +20%", lang)),
            StatKind::Range => format!("{} {}", name(*tower_type), pick2("射程 +15%", "range +15%", lang)),
            // SK6 grew the vocabulary; AC4 never offers these.
            StatKind::CritChance | StatKind::CritMultiplier => String::new(),
        },
        ChoiceKind::GoldBoost => pick2("击杀金币 +20%", "Kill gold +20%", lang).to_string(),
        ChoiceKind::GetTower { tower_type } => {
            pick2("获得 {}（入手牌）", "Get {} (into hand)", lang)
                .replace("{}", name(*tower_type))
        }
    }
}

/// Per-kind label for a fused tower's special trait line (info panel).
pub fn fusion_trait(kind: crate::components::FusionKind, lang: Lang) -> String {
    use crate::components::FusionKind;
    match kind {
        FusionKind::Archmage => pick2(
            "特技：AOE 爆炸（半径 3.0）",
            "Trait: AOE blast (radius 3.0)",
            lang,
        )
        .to_string(),
        FusionKind::Bastion => pick2(
            "特技：AOE + 减速30%（1秒）",
            "Trait: AOE + 30% slow (1s)",
            lang,
        )
        .to_string(),
        FusionKind::Marksman => pick2(
            "特技：优先攻击血量最高的怪",
            "Trait: targets the highest-HP enemy",
            lang,
        )
        .to_string(),
        FusionKind::Hybrid => pick2(
            "特技：混合伤害穿透物理护甲",
            "Trait: mixed damage pierces armor",
            lang,
        )
        .to_string(),
    }
}

pub fn attack_label(attack: AttackType, lang: Lang) -> &'static str {
    match attack {
        AttackType::Physical => ATTACK_PHYSICAL.pick(lang),
        AttackType::Magic => ATTACK_MAGIC.pick(lang),
        AttackType::Mixed => ATTACK_MIXED.pick(lang),
    }
}

// --- Persistence + toggle systems -------------------------------------------

/// Startup: load the persisted language preference (missing/corrupt -> Zh).
pub fn load_ui_prefs(mut lang: ResMut<Lang>, path: Res<UiPrefsPath>) {
    if let Ok(text) = std::fs::read_to_string(&path.0) {
        for line in text.lines() {
            if let Some((key, value)) = line.split_once('=') {
                if key.trim() == "lang" {
                    *lang = match value.trim() {
                        "en" => Lang::En,
                        _ => Lang::Zh,
                    };
                }
            }
        }
    }
    info!("[ui7] loaded lang = {:?}", *lang);
}

/// `L` toggles the UI language and persists the choice immediately.
pub fn toggle_lang(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<crate::resources::DebugPanelCapture>,
    mut lang: ResMut<Lang>,
    path: Res<UiPrefsPath>,
) {
    // Typing into a debug-panel field must not toggle the language (DB1).
    if panel.keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        *lang = lang.next();
        save_ui_prefs(&lang, &path);
        info!("[ui7] lang toggled to {:?}", *lang);
    }
}

pub fn save_ui_prefs(lang: &Lang, path: &UiPrefsPath) {
    let text = format!("lang={}\n", lang.code());
    if let Err(e) = std::fs::write(&path.0, text) {
        warn!("[ui7] lang save failed: {e}");
    }
}
