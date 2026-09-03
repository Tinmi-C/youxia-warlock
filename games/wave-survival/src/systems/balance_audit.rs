//! Headless balance readout — pure math, NOT a Bevy system.
//!
//! The "AI-friendly tuning" observation channel (docs/balance-anchors.md §4):
//! instead of "run the game and feel it", this module turns the current
//! definition tables + wave formulas into objective per-wave metrics so a
//! human or AI can read the difficulty directly and tune toward an anchor.
//!
//! It reads the SAME numbers the game uses (no separate model): the wave
//! formulas, `MonsterKind`/`WeaponKind` multiplier methods, the `Balance`
//! scales, and the contact-bite constants. It only *reads*; it touches no ECS
//! entities and no app. It is an analysis companion, not a gameplay system.

use crate::components::{MonsterKind, WeaponKind};
use crate::resources::Balance;
use crate::systems::contact; // CONTACT_DAMAGE / INVULN_TIME (bite throttle)
use crate::systems::wave::{kinds_for_wave, wave_hp, wave_speed, SPAWN_RADIUS};

/// Player full HP. `systems::player::spawn_player` uses `Hp::full(100.0)`;
/// there is no const to import yet, so this mirrors it (single source is
/// player.rs; keep in sync deliberately when the player HP becomes tunable).
pub const PLAYER_HP: f32 = 100.0;

/// Assumed count the 120° fan hits in one swing for the optimistic clear time.
/// The whole point is to make this estimate an explicit, changeable constant
/// rather than a hidden guess.
pub const MULTI_HIT_ESTIMATE: f32 = 2.0;

// --- Design target bands (docs/balance-anchors.md §5). --------------------
// These are the *settled* anchor values, kept here as data so the readout can
// flag OFF-target waves directly (the "对照锚点" part of the AI tuning loop).
// `None` = not yet decided / not statically computable (e.g. N1 survival wave,
// N5 bite count needs a playtest) — those stay Tbd in the report.
//
// The rest of each anchor is still a per-game design decision; the framework
// (mechanism) is generic, these specific bands are wave-survival's own.
/// N2 — a normal grunt should die in 2-3 hits (TTK ≤ this).
pub const GRUNT_TTK_MAX: f32 = 1.3;
/// N3 — the fastest monster must give a reaction window ≥ this (approach time).
pub const FAST_APPROACH_MIN: f32 = 1.1;
/// N4 — a tank's approach time should be ≥ this × its own TTK (slow enough to
/// be "an obstacle you can ignore" rather than "a fast threat"). Recalibrated
/// to 1.0: the old 1.2 demanded a 20% kill-before-reach margin that a tanky
/// slow unit can't hold as waves grow (the ratio decays, see the doc).
pub const TANK_RATIO_MIN: f32 = 1.0;

/// The playable window: waves ≤ this are the "comfortable" range the anchors
/// must hold in; waves after it are the intended difficulty ramp (you are
/// supposed to start losing), so beyond it every anchor reports `Tbd`, not
/// `Fail`. Calibrated so w5 (the last wave you can comfortably stand-and-fight
/// through) still passes, and w6+ is the ramp onset. Flip to 6 to include it.
pub const PLAYABLE_WINDOW: u32 = 5;

/// Outcome of one anchor check on one wave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorStatus {
    /// Within the settled target band.
    Pass,
    /// Outside the band (the readout is telling you to tune).
    Fail,
    /// Not computable statically / not yet decided.
    Tbd,
}

/// One anchor's verdict for a wave, with the value and target so an AI can see
/// exactly how far off it is.
#[derive(Debug, Clone)]
pub struct AnchorCheck {
    /// Anchor id, e.g. "N2 grunt_ttk".
    pub name: &'static str,
    /// The measured value (f32::NAN when Tbd).
    pub value: f32,
    /// Human-readable target, e.g. "≤1.3s".
    pub target: &'static str,
    pub status: AnchorStatus,
}

impl AnchorCheck {
    fn render(&self) -> String {
        let mark = match self.status {
            AnchorStatus::Pass => "OK ",
            AnchorStatus::Fail => "OFF",
            AnchorStatus::Tbd => "TBD",
        };
        match self.status {
            AnchorStatus::Tbd => format!("  {mark} {} (not decided / needs playtest)", self.name),
            _ => format!("  {mark} {} = {:.2} (target {})", self.name, self.value, self.target),
        }
    }
}

/// The weapon the readout assumes is equipped (the player's spawn weapon).
fn weapon_kind() -> WeaponKind {
    WeaponKind::IronSword
}

/// "What-if" multipliers so the readout can forecast a tuning change WITHOUT
/// editing game code: the AI (or a designer) answers "if I set tank speed to
/// X, does N4 fix itself?" before committing to a real change. `None` = use the
/// game's own multiplier as-is.
#[derive(Debug, Clone, Copy, Default)]
pub struct Overrides {
    pub grunt_hp_mul: Option<f32>,
    pub runner_speed_mul: Option<f32>,
    pub tank_speed_mul: Option<f32>,
    pub tank_hp_mul: Option<f32>,
}

impl Overrides {
    /// Effective HP multiplier for a kind: the override if set, else the table's.
    pub fn hp_mul(&self, kind: MonsterKind) -> f32 {
        match kind {
            MonsterKind::Grunt => self.grunt_hp_mul.unwrap_or_else(|| kind.hp_mul()),
            MonsterKind::Tank => self.tank_hp_mul.unwrap_or_else(|| kind.hp_mul()),
            _ => kind.hp_mul(),
        }
    }

    /// Effective speed multiplier for a kind: the override if set, else the table's.
    pub fn speed_mul(&self, kind: MonsterKind) -> f32 {
        match kind {
            MonsterKind::Runner => self.runner_speed_mul.unwrap_or_else(|| kind.speed_mul()),
            MonsterKind::Tank => self.tank_speed_mul.unwrap_or_else(|| kind.speed_mul()),
            _ => kind.speed_mul(),
        }
    }
}

/// Per-kind combat snapshot for one wave.
#[derive(Debug, Clone)]
pub struct KindReadout {
    pub kind: MonsterKind,
    pub count: u32,
    /// Per-entity HP this wave (wave base × kind multiplier).
    pub hp: f32,
    /// Chase speed this wave (wave base × kind multiplier).
    pub speed: f32,
    /// Time to kill one of this kind with single-target sustained DPS.
    pub ttk: f32,
    /// Time to cross the spawn ring at `speed`.
    pub approach: f32,
    /// `approach / ttk`. > 1 means you can kill it before it reaches you.
    pub approach_over_ttk: f32,
}

/// One wave's full readout.
#[derive(Debug, Clone)]
pub struct WaveReadout {
    pub n: u32,
    pub count: u32,
    pub total_hp: f32,
    /// Player single-target sustained DPS.
    pub player_dps: f32,
    /// `total_hp / player_dps` (conservative floor, no multi-hit).
    pub clear_single: f32,
    /// `total_hp / (player_dps * MULTI_HIT_ESTIMATE)` (the "real" pace).
    pub clear_multi: f32,
    /// Approach time of the fastest kind this wave (the one that reaches you).
    pub min_approach: f32,
    /// Worst-case seconds to die if constantly chewed (see §4 of the doc —
    /// contact is globally throttled to one bite per invuln, so swarm count
    /// does not raise incoming DPS).
    pub survival_seconds: f32,
    /// `PLAYER_HP / CONTACT_DAMAGE`.
    pub survival_bites: f32,
    pub kinds: Vec<KindReadout>,
}

impl WaveReadout {
    /// One line, sized for a log / CI readout.
    pub fn one_line(&self) -> String {
        format!(
            "wave {:>2}: {:>2} units, hp {:>5.0}, dps {:>5.0}, clear {:.2}s (x{} {:.2}s), min-approach {:.2}s, survive {:.1}s",
            self.n,
            self.count,
            self.total_hp,
            self.player_dps,
            self.clear_single,
            MULTI_HIT_ESTIMATE,
            self.clear_multi,
            self.min_approach,
            self.survival_seconds,
        )
    }
}

/// Player single-target sustained DPS from the equipped weapon + Balance scale:
/// `damage * slash_damage_scale / (cooldown * slash_cooldown_scale)`.
pub fn player_dps(balance: &Balance) -> f32 {
    let w = weapon_kind();
    w.damage() * balance.slash_damage_scale / (w.cooldown() * balance.slash_cooldown_scale)
}

/// Compute a single wave's readout from the same data the game spawns with.
pub fn wave_readout(n: u32, balance: &Balance) -> WaveReadout {
    wave_readout_with(n, balance, &Overrides::default())
}

/// Like [`wave_readout`], but with "what-if" multiplier overrides applied.
pub fn wave_readout_with(n: u32, balance: &Balance, o: &Overrides) -> WaveReadout {
    let base_hp = wave_hp(n);
    let base_speed = wave_speed(n);
    let kinds = kinds_for_wave(n);
    let dps = player_dps(balance);

    // Aggregate by kind, spawning order: grunt / runner / tank / elite.
    let mut per_kind: Vec<KindReadout> = Vec::new();
    for kind in [
        MonsterKind::Grunt,
        MonsterKind::Runner,
        MonsterKind::Tank,
        MonsterKind::Elite,
    ] {
        let count = kinds.iter().filter(|k| **k == kind).count() as u32;
        if count == 0 {
            continue;
        }
        let hp = base_hp * o.hp_mul(kind);
        let speed = base_speed * o.speed_mul(kind);
        let ttk = hp / dps;
        let approach = SPAWN_RADIUS / speed;
        per_kind.push(KindReadout {
            kind,
            count,
            hp,
            speed,
            ttk,
            approach,
            approach_over_ttk: approach / ttk,
        });
    }

    let total_hp = per_kind.iter().map(|k| k.hp * k.count as f32).sum();
    let survival_bites = PLAYER_HP / contact::CONTACT_DAMAGE;
    let survival_seconds = survival_bites * contact::INVULN_TIME;
    let min_approach = per_kind.iter().map(|k| k.approach).fold(f32::INFINITY, f32::min);

    WaveReadout {
        n,
        count: kinds.len() as u32,
        total_hp,
        player_dps: dps,
        clear_single: total_hp / dps,
        clear_multi: total_hp / (dps * MULTI_HIT_ESTIMATE),
        min_approach,
        survival_seconds,
        survival_bites,
        kinds: per_kind,
    }
}

/// Evaluate the (statically-settled) anchor bands against one wave's readout.
/// Returns one check per band; a band the wave can't exercise returns Tbd.
///
/// Waves beyond the playable window report every anchor as `Tbd` — they are
/// the intended difficulty ramp (the player is supposed to start losing), so
/// "Fail" there would be a false alarm, not an imbalance.
pub fn check_anchors(r: &WaveReadout) -> Vec<AnchorCheck> {
    if r.n > PLAYABLE_WINDOW {
        return vec![
            AnchorCheck { name: "N2 grunt_ttk", value: f32::NAN, target: "≤1.3s", status: AnchorStatus::Tbd },
            AnchorCheck { name: "N3 fast_approach", value: f32::NAN, target: "≥1.1s", status: AnchorStatus::Tbd },
            AnchorCheck { name: "N4 tank_ratio", value: f32::NAN, target: "≥1.0×", status: AnchorStatus::Tbd },
        ];
    }

    let mut checks = Vec::with_capacity(3);

    // N2 — grunt TTK.
    let grunt = r.kinds.iter().find(|k| k.kind == MonsterKind::Grunt);
    match grunt {
        Some(g) => checks.push(AnchorCheck {
            name: "N2 grunt_ttk",
            value: g.ttk,
            target: "≤1.3s",
            status: if g.ttk <= GRUNT_TTK_MAX {
                AnchorStatus::Pass
            } else {
                AnchorStatus::Fail
            },
        }),
        None => checks.push(AnchorCheck {
            name: "N2 grunt_ttk",
            value: f32::NAN,
            target: "≤1.3s",
            status: AnchorStatus::Tbd,
        }),
    }

    // N3 — fastest monster reaction window (any kind's shortest approach).
    checks.push(AnchorCheck {
        name: "N3 fast_approach",
        value: r.min_approach,
        target: "≥1.1s",
        status: if r.min_approach >= FAST_APPROACH_MIN {
            AnchorStatus::Pass
        } else {
            AnchorStatus::Fail
        },
    });

    // N4 — tank approach ≥ ratio × its TTK.
    match r.kinds.iter().find(|k| k.kind == MonsterKind::Tank) {
        Some(t) => checks.push(AnchorCheck {
            name: "N4 tank_ratio",
            value: t.approach_over_ttk,
            target: "≥1.0×",
            status: if t.approach_over_ttk >= TANK_RATIO_MIN {
                AnchorStatus::Pass
            } else {
                AnchorStatus::Fail
            },
        }),
        None => checks.push(AnchorCheck {
            name: "N4 tank_ratio",
            value: f32::NAN,
            target: "≥1.0×",
            status: AnchorStatus::Tbd,
        }),
    }

    checks
}

/// Where the difficulty wall sits, inferred rather than assumed: the first wave
/// whose *clear time* exceeds the contact survival time. Reported as a band
/// because the real clear pace depends on how many enemies the fan hits — so the
/// single-target floor and the ×MULTI_HIT_ESTIMATE ceiling bracket the truth.
/// Kiting and Nova push this higher (it is a *stand-and-fight* floor).
#[derive(Debug, Clone)]
pub struct SurvivalCeiling {
    /// First wave where `clear_single > survival_seconds`.
    pub single: Option<u32>,
    /// First wave where `clear_multi > survival_seconds`.
    pub multi: Option<u32>,
}

impl SurvivalCeiling {
    /// e.g. "5–6 (stand-and-fight)".
    pub fn describe(&self) -> String {
        match (self.single, self.multi) {
            (Some(a), Some(b)) if a != b => format!("{a}–{b}"),
            (Some(a), Some(_)) => format!("{a}"),
            (Some(a), None) => format!("≥{}, beyond tested range", a),
            (None, None) => "beyond tested range".to_string(),
            (None, Some(b)) => format!("{b}"),
        }
    }
}

/// First wave (within `max_wave`) where clear time passes the survival time,
/// evaluated separately for the single-target floor and multi-hit ceiling.
pub fn natural_survival_ceiling(max_wave: u32, balance: &Balance) -> SurvivalCeiling {
    natural_survival_ceiling_with(max_wave, balance, &Overrides::default())
}

/// Like [`natural_survival_ceiling`], with "what-if" overrides applied.
pub fn natural_survival_ceiling_with(max_wave: u32, balance: &Balance, o: &Overrides) -> SurvivalCeiling {
    let mut single = None;
    let mut multi = None;
    for n in 1..=max_wave {
        let r = wave_readout_with(n, balance, o);
        if single.is_none() && r.clear_single > r.survival_seconds {
            single = Some(n);
        }
        if multi.is_none() && r.clear_multi > r.survival_seconds {
            multi = Some(n);
        }
        if single.is_some() && multi.is_some() {
            break;
        }
    }
    SurvivalCeiling { single, multi }
}

/// A readable report for waves `1..=max_wave`: the metric line plus the
/// anchor verdicts, so a human or AI can read "which wave, which metric,
/// off by how much" in one place.
pub fn report_all_waves(max_wave: u32, balance: &Balance) -> String {
    report_all_waves_with(max_wave, balance, &Overrides::default())
}

/// Like [`report_all_waves`], with "what-if" overrides folded into every metric.
pub fn report_all_waves_with(max_wave: u32, balance: &Balance, o: &Overrides) -> String {
    let mut out = String::new();
    out.push_str("=== Wave Survival balance readout (IronSword, default Balance) ===\n");
    out.push_str(&format!(
        "playable window: waves ≤ {PLAYABLE_WINDOW} are judged against anchors; beyond that = difficulty ramp (Tbd)\n"
    ));
    let ceiling = natural_survival_ceiling_with(max_wave, balance, o);
    out.push_str(&format!(
        "natural survival ceiling (stand-and-fight, no kiting/Nova): wave ~{}\n\n",
        ceiling.describe()
    ));
    for n in 1..=max_wave {
        let r = wave_readout_with(n, balance, o);
        out.push_str(&r.one_line());
        out.push('\n');
        for c in check_anchors(&r) {
            out.push_str(&c.render());
            out.push('\n');
        }
        out.push('\n');
    }
    out
}
