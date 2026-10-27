//! LT2-5 tower bonds (v2 proposal §2 #5): a game-side RON table
//! (`maps/bonds.ron`) maps on-field tower-type counts to a StatMods grant
//! EVERY tower carries while the combination stands:
//! `"arrow" x2 + "ice" x1 -> all towers DamageMult +0.10` is the shipped
//! example. Each bond owns a dedicated marker entity as its StatMods source
//! (never the holder — same-source collisions are the known INV flaw), the
//! per-frame pass upserts entries while satisfied and strips them the moment
//! the count falls below the requirement (a sold tower leaves nothing
//! behind; a tower placed later joins on the next frame).
//!
//! Counts match def ids exactly — an upgraded tower counts as its new def
//! (arrow2 is not arrow). The table is data: bonds tune without code.

use bevy::prelude::*;
use warbase::prelude::*;

/// One bond row from maps/bonds.ron.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BondDef {
    pub id: String,
    /// (def id, minimum on-field count) pairs — ALL must hold.
    pub requires: Vec<(String, u32)>,
    pub kind: StatKind,
    pub amount: f32,
}

/// The parsed bond table.
#[derive(Resource, Debug, Clone, Default, serde::Deserialize)]
pub struct BondTable {
    pub bonds: Vec<BondDef>,
}

/// Marker entity per bond (parallel to `BondTable::bonds`) — the StatMods
/// source of that bond's entries.
#[derive(Resource, Debug, Default, Clone)]
pub struct BondSources(pub Vec<Entity>);

/// Path of the shipped bond table.
pub fn bonds_path() -> std::path::PathBuf {
    std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/maps/bonds.ron"))
}

/// Load the bond table (missing file = empty table, maps without bonds stay
/// playable) and spawn one marker source entity per bond.
pub fn load_bonds(world: &mut World) {
    let table: BondTable = match std::fs::read_to_string(bonds_path()) {
        Ok(text) => ron::from_str(&text).unwrap_or_else(|e| {
            panic!("[loop-td] bonds table {} failed to parse: {e}", bonds_path().display())
        }),
        Err(_) => BondTable::default(),
    };
    let sources = table.bonds.iter().map(|_| world.spawn_empty().id()).collect();
    world.insert_resource(table);
    world.insert_resource(BondSources(sources));
    info!("[loop-td] bonds loaded: {}", bonds_path().display());
}

/// Update: count Defender units per def, then for every bond either upsert
/// its entry onto every Defender unit (satisfied) or strip its source's
/// entries everywhere (not satisfied). Idempotent per frame.
pub fn apply_bonds(world: &mut World) {
    let Some(table) = world.get_resource::<BondTable>().cloned() else { return };
    let Some(sources) = world.get_resource::<BondSources>().cloned() else { return };

    // Count towers per def (BTreeMap: deterministic order, though the pass
    // is order-independent — each bond writes its own source only).
    let mut counts: std::collections::BTreeMap<String, u32> = Default::default();
    let mut towers: Vec<Entity> = Vec::new();
    {
        let mut q = world.query_filtered::<(Entity, &UnitInstance, &Allegiance), Without<Dead>>();
        for (e, inst, side) in q.iter(&*world) {
            if *side == Allegiance::Defender {
                *counts.entry(inst.def.clone()).or_default() += 1;
                towers.push(e);
            }
        }
    }

    for (i, bond) in table.bonds.iter().enumerate() {
        let Some(&source) = sources.0.get(i) else { continue };
        let satisfied = bond
            .requires
            .iter()
            .all(|(def, need)| counts.get(def).copied().unwrap_or(0) >= *need);
        for &tower in &towers {
            if satisfied {
                let Ok(mut holder) = world.get_entity_mut(tower) else { continue };
                let mut mods = holder.entry::<StatMods>().or_default();
                mods.into_mut().upsert(ModEntry {
                    source,
                    kind: bond.kind,
                    amount: bond.amount,
                    expires_at: None,
                    from_aura: false,
                });
            } else if let Some(mut mods) = world.get_mut::<StatMods>(tower) {
                mods.entries.retain(|e| e.source != source);
            }
        }
    }
}
