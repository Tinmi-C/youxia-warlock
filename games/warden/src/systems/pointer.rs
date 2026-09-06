//! Mouse-first interaction (UI1 + pointer place / point-select fusion).
//!   - A bottom shop bar (bevy_ui) shows the owned tower types (build mode) and
//!     the current shop offers (buy to unlock, AC2); it rebuilds automatically
//!     whenever the hand or the offer changes.
//!   - Left click on a slot places the armed tower (Intermission only).
//!   - Left click on a tower selects it as a fusion ingredient; clicking a second
//!     tower fuses them if they match a recipe.
//!   - Right click cancels build mode / clears fusion selection.
//! Keyboard hotkeys (1-4 / E / F / Space) remain as fallback.
//!
//! Split into three small systems: Bevy caps a single system at 16 params.

use std::collections::BTreeSet;

use bevy::input::mouse::MouseButton;
use bevy::prelude::*;
use bevy::window::Window;

use crate::components::{AttackType, FusionKind, ShopButton, Tower, TowerKind, TowerSlot};
use crate::resources::{
    BuildMode, Economy, FusionDefs, FusionSel, Hand, SelectedTower, ShopOffers, TowerDefs,
    WavePhase, WaveState,
};

/// Marker on the shop bar root so it can be despawned on rebuild (Bevy 0.19
/// despawn removes children recursively).
#[derive(Component)]
pub struct ShopBarRoot;

/// A bevy_ui shop offer button: buys (unlocks) that base tower type (AC2).
#[derive(Component)]
pub struct OfferButton {
    pub tower_index: usize,
}

/// Spawn the bottom shop bar: owned types row (UI1) + offers row (AC2).
fn build_shop_bar(commands: &mut Commands, defs: &TowerDefs, offers: &ShopOffers, hand: &Hand) {
    // Dedupe for display: AC3 drops may push a type that is already owned.
    let owned: Vec<usize> = hand
        .owned_towers
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    commands
        .spawn((
            ShopBarRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(12.0),
                left: Val::Px(12.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                ..default()
            },
        ))
        .with_children(|root| {
            // Row 1: owned types — click arms build mode (UI1).
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                for i in owned {
                    let Some(def) = defs.list.get(i) else {
                        continue;
                    };
                    row.spawn((
                        Button,
                        ShopButton { tower_index: i },
                        Interaction::None,
                        Node {
                            width: Val::Px(96.0),
                            height: Val::Px(48.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.2, 0.3, 0.4)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(format!("{} {}g", def.name, def.cost)),
                            TextFont {
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::WHITE),
                        ));
                    });
                }
            });
            // Row 2: shop offers — click buys (unlocks) the type (AC2).
            root.spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                ..default()
            })
            .with_children(|row| {
                for &i in &offers.offers {
                    let Some(def) = defs.list.get(i) else {
                        continue;
                    };
                    row.spawn((
                        Button,
                        OfferButton { tower_index: i },
                        Interaction::None,
                        Node {
                            width: Val::Px(96.0),
                            height: Val::Px(40.0),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.45, 0.35, 0.15)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(format!("[shop] {} {}g", def.name, def.cost)),
                            TextFont {
                                font_size: FontSize::Px(12.0),
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.9, 0.6)),
                        ));
                    });
                }
            });
        });
}

/// Rebuild the shop bar whenever the hand or the offer changes (AC1 deal /
/// AC2 refresh / AC3 drops / AC4 get-tower). Cached key avoids per-frame work.
pub fn refresh_shop_bar(
    mut commands: Commands,
    defs: Res<TowerDefs>,
    offers: Res<ShopOffers>,
    hand: Res<Hand>,
    mut cache: Local<(u32, usize)>,
    roots: Query<Entity, With<ShopBarRoot>>,
) {
    let key = (offers.version, hand.owned_towers.len());
    if *cache == key {
        return;
    }
    *cache = key;
    for root in &roots {
        commands.entity(root).despawn();
    }
    build_shop_bar(&mut commands, &defs, &offers, &hand);
}

/// A pressed shop button arms build mode with that tower type.
pub fn handle_shop_buttons(
    mut q: Query<(&Interaction, &ShopButton), Changed<Interaction>>,
    mut selected: ResMut<SelectedTower>,
    mut build: ResMut<BuildMode>,
) {
    for (inter, btn) in &mut q {
        if *inter == Interaction::Pressed {
            selected.tower_index = btn.tower_index;
            build.armed = true;
            info!("[mouse] build mode armed for tower {}", btn.tower_index);
        }
    }
}

/// AC2: clicking an offer buys (unlocks) that base tower type. The purchase
/// pays the tower cost once; each later placement of the unlocked type still
/// pays per build (TO2 rule unchanged).
pub fn handle_offer_buttons(
    mut q: Query<(&Interaction, &OfferButton), Changed<Interaction>>,
    defs: Res<TowerDefs>,
    mut economy: ResMut<Economy>,
    mut hand: ResMut<Hand>,
) {
    for (inter, btn) in &mut q {
        if *inter != Interaction::Pressed {
            continue;
        }
        if hand.owned_towers.contains(&btn.tower_index) {
            info!("[mouse] offer {}: already owned", btn.tower_index);
            continue;
        }
        let Some(def) = defs.list.get(btn.tower_index) else {
            continue;
        };
        if economy.gold < def.cost {
            info!(
                "[mouse] offer {}: not enough gold ({} < {})",
                def.name, economy.gold, def.cost
            );
            continue;
        }
        economy.gold -= def.cost;
        hand.owned_towers.push(btn.tower_index);
        info!(
            "[mouse] bought {} for {} (hand {:?})",
            def.name, def.cost, hand.owned_towers
        );
    }
}

/// Right click cancels build mode and clears the fusion selection.
pub fn mouse_cancel(
    buttons: Res<ButtonInput<MouseButton>>,
    mut build: ResMut<BuildMode>,
    mut fusion_sel: ResMut<FusionSel>,
) {
    if buttons.just_pressed(MouseButton::Right) {
        build.armed = false;
        fusion_sel.a = None;
        info!("[mouse] cleared build/fusion selection");
    }
}

/// Left click in build mode places the armed tower at the nearest slot.
pub fn mouse_place_system(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    selected: Res<SelectedTower>,
    defs: Res<TowerDefs>,
    wave: Res<WaveState>,
    hand: Res<Hand>,
    build: Res<BuildMode>,
    mut economy: ResMut<Economy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
) {
    if !build.armed || !buttons.just_pressed(MouseButton::Left) || wave.phase != WavePhase::Intermission
    {
        return;
    }
    let Some((cam_comp, cam_tf)) = camera.single().ok() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    let Some((slot_e, _)) = nearest_slot_screen(&slots, cursor, cam_comp, cam_tf) else {
        return;
    };
    let Some(def) = defs.list.get(selected.tower_index) else {
        return;
    };
    if !hand.owned_towers.contains(&selected.tower_index) || economy.gold < def.cost {
        return;
    }
    if let Ok((_, mut slot, stf)) = slots.get_mut(slot_e) {
        if slot.occupied {
            return;
        }
        slot.occupied = true;
        economy.gold -= def.cost;
        commands.spawn((
            Tower {
                tower_index: selected.tower_index,
                damage: def.damage,
                attack_speed: def.attack_speed,
                range: def.range,
                attack_type: def.attack_type,
                cooldown: 0.0,
                target: None,
                kind: TowerKind::Base(selected.tower_index),
                aoe_radius: 0.0,
                slow_factor: 0.0,
                slow_duration: 0.0,
            },
            Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
            MeshMaterial3d(materials.add(match def.attack_type {
                AttackType::Physical => Color::srgb(0.7, 0.7, 0.8),
                AttackType::Magic => Color::srgb(0.5, 0.4, 0.9),
                AttackType::Mixed => Color::srgb(0.6, 0.9, 0.6),
            })),
            Transform::from_xyz(stf.translation.x, 0.6, stf.translation.z),
        ));
        info!("[mouse] placed {} at slot, gold={}", def.name, economy.gold);
    }
}

/// Left click outside build mode selects a tower, or fuses the two selected.
pub fn mouse_fuse_system(
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    wave: Res<WaveState>,
    build: Res<BuildMode>,
    mut fusion_sel: ResMut<FusionSel>,
    fusion_defs: Res<FusionDefs>,
    defs: Res<TowerDefs>,
    mut economy: ResMut<Economy>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut slots: Query<(Entity, &mut TowerSlot, &Transform)>,
    towers: Query<(Entity, &Tower, &Transform)>,
) {
    if build.armed || !buttons.just_pressed(MouseButton::Left) || wave.phase != WavePhase::Intermission
    {
        return;
    }
    let Some((cam_comp, cam_tf)) = camera.single().ok() else {
        return;
    };
    let Some(cursor) = windows.single().ok().and_then(|w| w.cursor_position()) else {
        return;
    };
    let Some(tower_e) = tower_at_screen(&towers, cursor, cam_comp, cam_tf) else {
        return;
    };
    match fusion_sel.a {
        None => {
            fusion_sel.a = Some(tower_e);
            info!("[mouse] selected tower for fusion");
        }
        Some(prev) if prev == tower_e => {
            fusion_sel.a = None;
            info!("[mouse] deselected tower");
        }
        Some(prev) => {
            if let Some(name) = try_fuse_two(
                prev,
                tower_e,
                &*fusion_defs,
                &*defs,
                &towers,
                &mut *economy,
                &mut commands,
                &mut *meshes,
                &mut *materials,
                &mut slots,
            ) {
                fusion_sel.a = None;
                info!("[mouse] fused -> {name}");
            } else {
                fusion_sel.a = Some(tower_e);
                info!("[mouse] pair not a recipe, reselected");
            }
        }
    }
}

/// Nearest slot to the screen-space cursor position.
fn nearest_slot_screen(
    slots: &Query<(Entity, &mut TowerSlot, &Transform)>,
    cursor: Vec2,
    cam: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter() {
        let Ok(sp) = cam.world_to_viewport(cam_tf, stf.translation) else {
            continue;
        };
        let d = sp.distance(cursor);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}

/// The tower whose screen position is nearest the cursor (within a threshold).
fn tower_at_screen(
    towers: &Query<(Entity, &Tower, &Transform)>,
    cursor: Vec2,
    cam: &Camera,
    cam_tf: &GlobalTransform,
) -> Option<Entity> {
    let threshold = 45.0;
    let mut best: Option<(Entity, f32)> = None;
    for (eid, _tower, ttf) in towers.iter() {
        let Ok(sp) = cam.world_to_viewport(cam_tf, ttf.translation) else {
            continue;
        };
        let d = sp.distance(cursor);
        if d <= threshold && best.map_or(true, |(_, bd)| d < bd) {
            best = Some((eid, d));
        }
    }
    best.map(|(e, _)| e)
}

/// Fuse two specific towers if they match a recipe. Deducts the 20% fee, frees
/// both ingredient slots and occupies one for the result. Returns the recipe name.
fn try_fuse_two(
    a: Entity,
    b: Entity,
    fusion_defs: &FusionDefs,
    defs: &TowerDefs,
    towers: &Query<(Entity, &Tower, &Transform)>,
    economy: &mut Economy,
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
) -> Option<&'static str> {
    let (kind_a, pos_a) = base_kind_pos(towers, a)?;
    let (kind_b, pos_b) = base_kind_pos(towers, b)?;
    let recipe = fusion_defs.list.iter().find(|r| {
        let mut need = r.ingredients.to_vec();
        for k in [kind_a, kind_b] {
            if let Some(i) = need.iter().position(|&x| x == k) {
                need.remove(i);
            }
        }
        need.is_empty()
    })?;
    let cost_ing = defs.list[recipe.ingredients[0]].cost + defs.list[recipe.ingredients[1]].cost;
    let fee = (cost_ing as f32 * recipe.fee_ratio).round() as u32;
    if economy.gold < fee {
        return None;
    }
    economy.gold -= fee;

    for eid in [a, b] {
        commands.entity(eid).despawn();
    }
    // Free both ingredient slots, then occupy one for the result.
    if let Some((sa, _)) = nearest_slot_world(slots, pos_a) {
        set_occupied(slots, sa, false);
    }
    if let Some((sb, _)) = nearest_slot_world(slots, pos_b) {
        set_occupied(slots, sb, false);
    }
    let (result_slot, result_pos) = nearest_slot_world(slots, pos_a)?;
    set_occupied(slots, result_slot, true);

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
        Transform::from_xyz(result_pos.x, 0.6, result_pos.z),
    ));
    Some(recipe.name)
}

/// (base_kind, world_pos) for a Base tower, or None for a fused tower.
fn base_kind_pos(
    towers: &Query<(Entity, &Tower, &Transform)>,
    eid: Entity,
) -> Option<(usize, Vec3)> {
    towers.get(eid).ok().and_then(|(_, t, tf)| match t.kind {
        TowerKind::Base(i) => Some((i, tf.translation)),
        _ => None,
    })
}

fn nearest_slot_world(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
    pos: Vec3,
) -> Option<(Entity, Vec3)> {
    let mut best: Option<(Entity, f32, Vec3)> = None;
    for (eid, _slot, stf) in slots.iter() {
        let d = stf.translation.distance(pos);
        if best.map_or(true, |(_, bd, _)| d < bd) {
            best = Some((eid, d, stf.translation));
        }
    }
    best.map(|(e, _, p)| (e, p))
}

fn set_occupied(
    slots: &mut Query<(Entity, &mut TowerSlot, &Transform)>,
    ent: Entity,
    occupied: bool,
) {
    if let Ok((_, mut slot, _)) = slots.get_mut(ent) {
        slot.occupied = occupied;
    }
}
