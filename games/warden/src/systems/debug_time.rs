//! Debug time control + info overlays (DB3, capability card "时间控制 + 信息叠层").
//!
//! Whole module `#[cfg(debug_assertions)]` like the rest of the debug core
//! (see `systems/mod.rs`): release builds have none of this.
//!
//! **Coexistence with `GameState::Paused`** (the card's noted tradeoff): the
//! player pause (P key) is *state-level* — gameplay systems stop running via
//! `run_if(in_state(GameState::Playing))`. The debug pause here is
//! *time-level* — it freezes `Time<Virtual>`, so systems still run but
//! observe a zero delta. Consequences:
//!   - the debug freeze works in any state and never touches the state
//!     machine (P stays the player pause; both freeze the world; they
//!     compose);
//!   - non-time actions still work while frozen (selecting towers, opening
//!     panels) which is exactly what inspecting a frozen frame wants;
//!   - stepping only makes sense in `Playing` (elsewhere the systems are
//!     state-gated and observe nothing anyway).
//!
//! Frame contract: bevy advances the virtual clock in `First` (before any
//! Update system), so whatever pause/speed `debug_time_clock` writes takes
//! effect on the NEXT frame. A step therefore lets exactly ONE frame through
//! — the frame after the action lands.

use bevy::prelude::*;

use crate::components::{Enemy, Projectile, Slow, Tower};
use crate::resources::DebugPanelCapture;
use crate::systems::debug::{DebugAction, DebugQueue};

/// Debug time state (DB3). Mutated only through `DebugAction`s (panel buttons
/// and hotkeys push the same intents; the cheat executor applies them).
#[derive(Resource, Debug)]
pub struct DebugTime {
    /// Freeze the virtual clock (time-level pause, see module docs).
    pub paused: bool,
    /// Debug 4x fast-forward (dev-only; the official 2x is a separate future
    /// card — this one never ships and does not join balance decisions).
    pub speed_4x: bool,
    /// A running step budget (advances a little, then re-pauses).
    pub step: DebugStep,
}

impl Default for DebugTime {
    fn default() -> Self {
        Self {
            paused: false,
            speed_4x: false,
            step: DebugStep::None,
        }
    }
}

/// What a step is currently advancing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DebugStep {
    /// No step pending.
    None,
    /// Let `n` more frames run, then re-pause (frames that actually ran with
    /// a non-zero delta are counted down).
    Frames(u32),
    /// Let virtual time run until `secs` has elapsed, then re-pause.
    Secs(f32),
}

/// Debug overlay toggles (DB3): observation layers on the live board.
#[derive(Resource, Debug, Default)]
pub struct DebugOverlays {
    /// A ground range ring under EVERY tower.
    pub show_ranges: bool,
    /// A floating hp / speed / slow plate over every enemy.
    pub show_enemy_stats: bool,
}

/// One-word clock status for the HUD strip / panel.
impl DebugTime {
    pub fn status(&self) -> &'static str {
        if !matches!(self.step, DebugStep::None) {
            "STEPPING"
        } else if self.paused {
            "PAUSED"
        } else if self.speed_4x {
            "4x"
        } else {
            "1x"
        }
    }
}

/// Hotkeys (DB3): F6 pause toggle · F7 step one frame · F8 step 0.1s · F9 4x
/// speed toggle. They push the exact same `DebugAction`s the F1 panel buttons
/// push — one code path for panel, keys and tests.
pub fn debug_time_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    mut queue: ResMut<DebugQueue>,
) {
    // Typing into a debug-panel text field must not fire the hotkeys (DB1).
    if panel.keyboard {
        return;
    }
    if keys.just_pressed(KeyCode::F6) {
        queue.0.push(DebugAction::TimePauseToggle);
    }
    if keys.just_pressed(KeyCode::F7) {
        queue.0.push(DebugAction::TimeStepFrame);
    }
    if keys.just_pressed(KeyCode::F8) {
        queue.0.push(DebugAction::TimeStepTenths);
    }
    if keys.just_pressed(KeyCode::F9) {
        queue.0.push(DebugAction::TimeSpeedToggle4x);
    }
}

/// The single writer of `Time<Virtual>` (DB3). Runs every frame right after
/// the cheat executor, ungated by game state — time control is orthogonal to
/// the state machine (see module docs). Pause/speed written here take effect
/// next frame (bevy advances the virtual clock in `First`).
pub fn debug_time_clock(
    time: Res<Time>,
    mut virt: ResMut<Time<Virtual>>,
    mut dt: ResMut<DebugTime>,
) {
    let delta = time.delta_secs();
    // Spend this frame's delta against a running step budget; a spent budget
    // re-engages the pause from the next frame on.
    match dt.step {
        DebugStep::Frames(n) if delta > 0.0 => {
            dt.step = if n <= 1 {
                DebugStep::None
            } else {
                DebugStep::Frames(n - 1)
            };
        }
        DebugStep::Secs(remaining) => {
            let remaining = remaining - delta;
            dt.step = if remaining <= 0.0 {
                DebugStep::None
            } else {
                DebugStep::Secs(remaining)
            };
        }
        _ => {}
    }
    let stepping = !matches!(dt.step, DebugStep::None);
    let frozen = dt.paused && !stepping;
    if frozen {
        virt.pause();
    } else {
        virt.unpause();
    }
    virt.set_relative_speed(if dt.speed_4x { 4.0 } else { 1.0 });
}

// --- Overlay: all-tower range rings -------------------------------------------

/// Marker on one tower's debug range ring (DB3).
#[derive(Component)]
pub struct DebugRangeRing;

/// A ground ring under every tower while `show_ranges` is on (DB3). Rebuilds
/// when the toggle flips or the tower set changes (towers appear/vanish);
/// toggle-off despawns every ring. The radius is the tower's base range —
/// same convention as the selection ring / build preview (UI9/UI10).
#[allow(clippy::type_complexity)]
pub fn debug_range_rings(
    mut commands: Commands,
    overlays: Res<DebugOverlays>,
    towers: Query<(Entity, &Tower, &Transform)>,
    rings: Query<Entity, With<DebugRangeRing>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: Local<(bool, Vec<Entity>)>, // (toggle, covered tower ids)
) {
    let mut tower_ids: Vec<Entity> = towers.iter().map(|(e, _, _)| e).collect();
    tower_ids.sort_unstable_by_key(|e| e.to_bits());
    let tower_count = tower_ids.len();
    let key = (overlays.show_ranges, tower_ids);
    if *cache == key {
        return;
    }
    *cache = key;
    for r in &rings {
        commands.entity(r).despawn();
    }
    if !overlays.show_ranges {
        return;
    }
    for (_, tower, tf) in towers.iter() {
        commands.spawn((
            DebugRangeRing,
            Mesh3d(meshes.add(Circle::new(tower.range))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.55, 0.15, 0.16),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                ..default()
            })),
            Transform::from_xyz(tf.translation.x, 0.06, tf.translation.z)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        ));
    }
    info!(
        "[db3] range rings {} ({} towers)",
        if overlays.show_ranges { "on" } else { "off" },
        tower_count
    );
}

// --- Overlay: enemy stat plates -------------------------------------------------

/// Marker on one enemy's floating debug plate (a child of the enemy, DB3).
#[derive(Component)]
pub struct DebugEnemyStats {
    pub enemy: Entity,
}

/// A floating `Text2d` plate over every enemy while `show_enemy_stats` is on
/// (DB3): `hp cur/max · v <world speed> · slow` — the slow line shows the
/// live factor and remaining seconds. Plates are children, so they despawn
/// with their enemy; toggle-off despawns them all.
pub fn debug_enemy_stats(
    mut commands: Commands,
    overlays: Res<DebugOverlays>,
    enemies: Query<(Entity, &Enemy)>,
    slows: Query<&Slow>,
    mut plates: Query<(Entity, &DebugEnemyStats, &mut Text2d, &mut Visibility)>,
) {
    if !overlays.show_enemy_stats {
        for (eid, _, _, _) in &plates {
            commands.entity(eid).despawn();
        }
        return;
    }
    // Ensure every enemy has a plate (spawned with its first text already
    // filled — the query above can't see this frame's deferred spawns).
    for (eid, enemy) in &enemies {
        if !plates.iter().any(|(_, plate, _, _)| plate.enemy == eid) {
            let line = stat_line(enemy, slows.get(eid).ok());
            commands.entity(eid).with_children(|parent| {
                parent.spawn((
                    DebugEnemyStats { enemy: eid },
                    Text2d::new(line),
                    TextFont {
                        font_size: FontSize::Px(11.0),
                        ..default()
                    },
                    TextColor(Color::srgb(1.0, 0.92, 0.55)),
                    Visibility::Visible,
                    // Tilt toward the 3/4 camera so the plate reads upright.
                    Transform::from_xyz(0.0, 1.2, 0.0)
                        .with_rotation(Quat::from_rotation_x(-0.67)),
                ));
            });
        }
    }
    // Keep the text live.
    for (_, plate, mut text, mut vis) in &mut plates {
        let Ok((_, enemy)) = enemies.get(plate.enemy) else {
            continue;
        };
        let line = stat_line(enemy, slows.get(plate.enemy).ok());
        if text.0 != line {
            text.0 = line;
        }
        *vis = Visibility::Visible;
    }
}

/// One plate's line: `hp cur/max · v <world speed> · slow`.
fn stat_line(enemy: &Enemy, slow: Option<&Slow>) -> String {
    let slow_line = slow
        .map(|s| format!(" slow x{:.2} {:.1}s", s.factor, s.timer))
        .unwrap_or_default();
    format!(
        "hp {:.0}/{:.0} v {:.2}{}",
        enemy.hp, enemy.max_hp, enemy.speed, slow_line
    )
}

// --- Debug HUD strip -------------------------------------------------------------

/// Marker on the always-on debug HUD strip (DB3): clock status + entity
/// counts (the panel may be closed; this stays observable).
#[derive(Component)]
pub struct DebugHudText;

/// Top-right one-liner (English — dev tool, not l10n): clock state, live
/// projectile count, enemy count. Spawned once, updated in place.
pub fn debug_hud_strip(
    mut commands: Commands,
    font: Res<crate::systems::hud::UiFont>,
    time: Res<DebugTime>,
    enemies: Query<(), With<Enemy>>,
    projectiles: Query<(), With<Projectile>>,
    mut strip: Query<(Entity, &mut Text), With<DebugHudText>>,
) {
    let line = format!(
        "[db3] time:{} proj={} enemies={}",
        time.status(),
        projectiles.iter().count(),
        enemies.iter().count(),
    );
    if let Ok((_, mut text)) = strip.single_mut() {
        if text.0 != line {
            text.0 = line;
        }
        return;
    }
    if strip.is_empty() {
        commands.spawn((
            DebugHudText,
            Text::new(line),
            TextFont {
                font: font.0.clone(),
                font_size: FontSize::Px(12.0),
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.85, 0.4)),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                right: Val::Px(10.0),
                ..default()
            },
        ));
    }
}
