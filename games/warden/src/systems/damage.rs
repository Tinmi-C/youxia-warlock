//! Unified damage resolution (SK7, ADR-0011 layer 4) — WX6-4b: the
//! pipeline (DamageRequest -> resolve_damage single-writer -> DamageDealt/
//! UnitDied) is the BASE's; this module carries the warden semantics
//! bridge (one-axis armor as a base AttackArmorTable, attack-channel
//! mapping) plus the game-side reactions that ride the base's messages
//! (hit flash on DamageDealt, death rewards on UnitDied in enemy.rs).

use bevy::prelude::*;

use crate::components::{AttackType, Enemy};

// WX6-4b: the damage MESSAGES and the resolver are the base's; warden
// keeps only the semantics bridge (armor mapping + table) and its
// game-side reactions (hit flash, death rewards).
pub use warbase::prelude::DamageDealt;
pub use warbase::prelude::DamageOrigin;
pub use warbase::prelude::DamageRequest;

/// The base's skill attribution index for a skill id (SKILL_ORDER row).
pub fn skill_row(skill: &str) -> usize {
    crate::resources::SKILL_ORDER
        .iter()
        .position(|s| *s == skill)
        .expect("skill id in SKILL_ORDER")
}

/// FX2 bridge: the base's resolver publishes DamageDealt; warden's hit
/// flash rides it (white flash on every applied hit, as before).
pub fn flash_hits(
    mut dealt: MessageReader<DamageDealt>,
    mut commands: Commands,
) {
    for d in dealt.read() {
        commands.entity(d.target).insert(crate::systems::fx::HitFlash {
            timer: crate::systems::fx::HIT_FLASH_SECS,
            original: None,
        });
    }
}

/// One-axis armor (§6), table-driven since WX6-4a: the warden semantics
/// (physical halved vs armored; magic/mixed bypass) ride the BASE's
/// attack-x-armor matrix — Normal/Fortified is the only 0.5 cell, every
/// other combo 1.0. The table becomes map data with warden.ron (WX6-6).
pub fn warden_armor_table() -> warbase::prelude::AttackArmorTable {
    let mut table = warbase::prelude::AttackArmorTable::default();
    table.mult[warbase::prelude::AttackType::Normal as usize]
        [warbase::prelude::ArmorType::Fortified as usize] = 0.5;
    table
}

/// Map warden's three attack channels onto base attack types (distinct rows
/// so readings/UI keep the three identities; only Normal meets the 0.5 cell).
pub fn to_base_attack(attack: AttackType) -> warbase::prelude::AttackType {
    match attack {
        AttackType::Physical => warbase::prelude::AttackType::Normal,
        AttackType::Magic => warbase::prelude::AttackType::Magic,
        AttackType::Mixed => warbase::prelude::AttackType::Piercing,
    }
}

/// Armor component for an enemy (physical armor => Fortified, else Light).
pub fn base_armor(physical_armor: bool) -> warbase::prelude::ArmorType {
    if physical_armor {
        warbase::prelude::ArmorType::Fortified
    } else {
        warbase::prelude::ArmorType::Light
    }
}
