//! Player input for the closed loop. One-card workhorse:
//!   - 1/2/3 picks the pending "三选一" option (AC4)
//!   - Space starts the next wave (Intermission only)
//!   - F fuses the first matching pair (TO5 legacy trigger; retires with UI11)
//! Tower selection/build moved to the command card (UI10, requirements §16):
//! the old WASD cursor and the 1-4/E keyboard flow are retired.

use std::collections::VecDeque;

use bevy::prelude::*;

use crate::components::{FusionKind, Tower, TowerKind, TowerSlot};
use crate::resources::{
    Boosts, ChoiceKind, Economy, FusionDefs, Hand, StatKind, TowerDefs, WaveChoice,
    WavePhase, WaveSchedule, WaveState,
};
use crate::states::GameState;

/// 1/2/3 picks the pending "三选一" option (AC4) and applies its effect.
pub fn choose_choice(
    keys: Res<ButtonInput<KeyCode>>,
    mut choice: ResMut<WaveChoice>,
    mut boosts: ResMut<Boosts>,
    mut hand: ResMut<Hand>,
) {
    if !choice.pending {
        return;
    }
    let idx = if keys.just_pressed(KeyCode::Digit1) {
        Some(0)
    } else if keys.just_pressed(KeyCode::Digit2) {
        Some(1)
    } else if keys.just_pressed(KeyCode::Digit3) {
        Some(2)
    } else {
        None
    };
    let Some(i) = idx else {
        return;
    };
    apply_choice(&mut choice, &mut boosts, &mut hand, i);
}

/// Shared choice application for the digit keys and the UI choice cards (UI2).
pub fn apply_choice(
    choice: &mut WaveChoice,
    boosts: &mut Boosts,
    hand: &mut Hand,
    i: usize,
) {
    let Some(opt) = choice.options.get(i) else {
        return;
    };
    // Clone the kind out so we can mutate `choice` and still log it.
    let kind = opt.kind.clone();
    match &kind {
        ChoiceKind::StatBoost { tower_type, stat } => match stat {
            StatKind::Damage => boosts.damage_mult[*tower_type] *= 1.2,
            StatKind::AttackSpeed => boosts.attack_speed_mult[*tower_type] *= 1.2,
            StatKind::Range => boosts.range_mult[*tower_type] *= 1.15,
        },
        ChoiceKind::GoldBoost => {
            boosts.kill_mult *= 1.2;
        }
        ChoiceKind::GetTower { tower_type } => {
            if !hand.owned_towers.contains(tower_type) {
                hand.owned_towers.push(*tower_type);
            }
        }
    }
    choice.pending = false;
    info!("[input] chose {:?}", kind);
}

/// Space begins the next wave (only from Intermission). Delegates to the shared
/// `try_start_wave` so the on-screen button (UI2) behaves identically.
pub fn start_next_wave(
    keys: Res<ButtonInput<KeyCode>>,
    mut wave: ResMut<WaveState>,
    schedule: Res<WaveSchedule>,
    choice: Res<WaveChoice>,
    state: Res<State<GameState>>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }
    try_start_wave(&mut wave, &schedule, &choice, &state);
}

/// Shared wave-start logic for the Space key and the "开始下一波" button (UI2).
/// Returns true when the wave actually started.
pub fn try_start_wave(
    wave: &mut WaveState,
    schedule: &WaveSchedule,
    choice: &WaveChoice,
    state: &State<GameState>,
) -> bool {
    if wave.phase != WavePhase::Intermission
        || choice.pending
        || *state.get() != GameState::Playing
    {
        return false;
    }
    if (wave.current as usize) >= schedule.waves.len() {
        return false;
    }
    let wave_def = &schedule.waves[wave.current as usize];
    let mut q = VecDeque::new();
    for entry in &wave_def.entries {
        for _ in 0..entry.count {
            q.push_back(entry.enemy_index);
        }
    }
    wave.spawn_queue = q;
    wave.phase = WavePhase::Combat;
    wave.active = 0;
    wave.spawn_timer = 0.0;
    info!("[wave] wave {} started", wave.current + 1);
    true
}

/// F fuses the first matching pair of field towers (TO5). Two ingredient towers
/// (each occupying a slot) are removed; a result tower occupies one slot, so one
/// buildable slot is freed. The fee is 20% of the ingredient total cost.
pub fn fuse_towers(
    keys: Res<ButtonInput<KeyCode>>,
    fusion: Res<FusionDefs>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    state: Res<State<GameState>>,
    mut economy: ResMut<Economy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    towers: Query<(Entity, &Tower, &Transform)>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
) {
    if !keys.just_pressed(KeyCode::KeyF)
        || wave.phase != WavePhase::Intermission
        || *state.get() != GameState::Playing
    {
        return;
    }
    for recipe in &fusion.list {
        // Match field towers against the recipe's ingredient multiset.
        let mut remaining = recipe.ingredients.to_vec();
        let mut found: Vec<(Entity, Vec3)> = Vec::new();
        for (eid, tower, ttf) in &towers {
            let TowerKind::Base(base_idx) = tower.kind else {
                continue;
            };
            if let Some(pos) = remaining.iter().position(|&r| r == base_idx) {
                remaining.remove(pos);
                found.push((eid, ttf.translation));
            }
        }
        if found.len() != 2 || !remaining.is_empty() {
            continue;
        }
        let cost_ing = defs.list[recipe.ingredients[0]].cost + defs.list[recipe.ingredients[1]].cost;
        let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
        if economy.gold < fee {
            info!("[fusion] not enough gold ({}) for {}", economy.gold, recipe.name);
            continue;
        }
        economy.gold -= fee;

        // Remove both ingredients and free their slots.
        for (eid, pos) in &found {
            commands.entity(*eid).despawn();
            if let Some((slot_e, _)) = nearest_slot(&mut slots, *pos) {
                set_occupied(&mut slots, slot_e, false);
            }
        }
        // Occupy the nearest slot to the first ingredient; spawn the result there.
        let anchor = found[0].1;
        let Some((anchor_slot, _)) = nearest_slot(&mut slots, anchor) else {
            continue;
        };
        set_occupied(&mut slots, anchor_slot, true);
        commands.spawn((
            Tower {
                tower_index: recipe.ingredients[0],
                damage: recipe.damage,
                attack_speed: recipe.attack_speed,
                range: recipe.range,
                attack_type: recipe.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Fused(recipe.kind),
                aoe_radius: recipe.aoe_radius,
                slow_factor: recipe.slow_factor,
                slow_duration: recipe.slow_duration,
            },
            Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
            MeshMaterial3d(materials.add(match recipe.kind {
                FusionKind::Marksman => Color::srgb(0.2, 0.8, 1.0),
                FusionKind::Archmage => Color::srgb(0.7, 0.2, 1.0),
                FusionKind::Hybrid => Color::srgb(0.2, 0.9, 0.4),
                FusionKind::Bastion => Color::srgb(1.0, 0.6, 0.1),
            })),
            Transform::from_xyz(anchor.x, 0.6, anchor.z),
        ));
        info!("[fusion] created {} fee={}", recipe.name, fee);
        return; // one fusion per key press
    }
}

/// Nearest slot entity+position to `pos` (by XZ distance from the given point).
fn nearest_slot(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
    pos: Vec3,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter_mut() {
        let d = stf.translation.distance(pos);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}

/// Set a slot's occupied flag.
fn set_occupied(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
    ent: Entity,
    occupied: bool,
) {
    if let Ok((_, mut slot, _)) = slots.get_mut(ent) {
        slot.occupied = occupied;
    }
}
