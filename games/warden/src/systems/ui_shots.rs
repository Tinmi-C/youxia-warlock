//! Engine-side skin screenshot mode (`--ui-shots [default|plan-a]`, STAB):
//! a frame-scripted run of the REAL windowed game that stages the bottom
//! bar states and captures each as a GPU screenshot — the port of loop-td's
//! `--ui-mock` pattern to warden. The run self-exits — the before/after
//! skin comparison images (`docs/img/skin/{default,plan-a}/`) are produced
//! by two unattended commands instead of hand-timed F12 presses.
//!
//! Lock-screen caveat (2026-10-28, empirically established): while the
//! session is LOCKED, macOS composites nothing into window drawables and
//! `Screenshot::primary_window()` captures black — reproduced on this
//! machine with loop-td's proven `--ui-mock v2` as well, so it is a
//! machine/OS state, not a warden bug. The lock-proof producer is the
//! OFFSCREEN variant of this script: `tests/ui_shots.rs` (bevy
//! `headless_renderer` pattern, no window at all — the MMP c59bda9
//! approach). This CLI remains the interactive/user-present path; the
//! staging helpers (`solo_bottom_bar`) are shared with the test.
//!
//! What gets staged (all through the real gateways — `place_towers`, the
//! `Selection` machine, `WaveDirector::force_begin`):
//!   f2    gold 120 + two archers (SK kit + a Fuse pair)
//!   f48   01-bar-build           full HUD, build card (Selection::None)
//!   f60   select the first archer
//!   f72   02-card-selected-tower sell/fuse/upgrade + skill chips
//!   f84   solo the command card  (plaque/report/info/control hidden)
//!   f96   03-card-closeup
//!   f108  solo the info zone
//!   f120  04-info-zone-closeup   (tower attributes)
//!   f132  restore + start wave 1 (real combat)
//!   f144  05-card-combat-lock    build card greyed, live fight
//!   f156  settle the wave, back to intermission, Selection::None
//!   f192  06-tooltip-hover       real hover: pinned cursor over a build button
//!   f220  AppExit::Success
//!
//! Debug-only tooling (same `#[cfg(debug_assertions)]` gate as DB1-3): normal
//! and release runs register nothing. Arming marks the run (`DebugRun`) so no
//! meta payout / meta_save.txt write happens.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::ui::{ComputedNode, UiGlobalTransform};

use crate::components::{Enemy, Tower};
use crate::resources::{DebugRun, Economy, Selection};
use crate::systems::command::{
    CommandAction, CommandButton, CommandCardRoot, ControlZoneRoot, InfoZoneRoot,
};
use crate::systems::debug::{place_towers, ShotSkin, free_slots_in_order};
use crate::systems::hud::TopPlaque;
use warbase::prelude::WaveDirector;

/// Script state (present = armed; every system here is a no-op without it).
#[derive(Resource, Debug)]
pub struct UiShotsState {
    pub frames: u32,
    pub skin: ShotSkin,
    /// The staged archer (the selected-tower face); `None` until staging.
    pub hero: Option<Entity>,
}

impl UiShotsState {
    pub fn new(skin: ShotSkin) -> Self {
        Self { frames: 0, skin, hero: None }
    }
}

/// The shot table: (frame, file stem). Frames strictly increase; every stem
/// is unique. Pinned by tests (`shot_table_covers_the_required_kinds`).
pub const SHOTS: [(u32, &str); 6] = [
    (48, "01-bar-build"),
    (72, "02-card-selected-tower"),
    (96, "03-card-closeup"),
    (120, "04-info-zone-closeup"),
    (144, "05-card-combat-lock"),
    (192, "06-tooltip-hover"),
];

const STAGE: u32 = 2;
const SELECT_HERO: u32 = 60;
const SOLO_CARD: u32 = 84;
const SOLO_INFO: u32 = 108;
const COMBAT: u32 = 132;
const SETTLE: u32 = 156;
/// Tooltip window: the pinned cursor must hold over the build button from
/// here until the shot (and a few frames past it) so hover + tooltip spawn.
const PIN_FROM: u32 = 157;
const PIN_TO: u32 = 200;
const EXIT: u32 = 220;

/// Output root: `<crate>/docs/img/skin/<skin>/`. `CARGO_MANIFEST_DIR` keeps
/// it stable regardless of the working directory cargo run started from.
pub fn out_dir(skin: ShotSkin) -> String {
    format!("{}/docs/img/skin/{}", env!("CARGO_MANIFEST_DIR"), skin.dir_name())
}

/// The frame driver (exclusive: staging needs the world-level scenario
/// helpers). Runs every Update frame; no-ops unless `UiShotsState` exists.
pub fn ui_shots_driver(world: &mut World) {
    let Some(mut state) = world.remove_resource::<UiShotsState>() else {
        return;
    };
    state.frames += 1;
    let f = state.frames;

    match f {
        STAGE => stage(world, &mut state),
        SELECT_HERO => select_hero(world, &mut state),
        SOLO_CARD => solo_bottom_bar(world, Some(SoloTarget::Card)),
        SOLO_INFO => solo_bottom_bar(world, Some(SoloTarget::InfoZone)),
        COMBAT => {
            solo_bottom_bar(world, None);
            *world.resource_mut::<Selection>() = Selection::None;
            // Real combat through the director (WaveState is a mirror —
            // writing it directly gets flattened by sync_wave_state).
            world.resource_mut::<WaveDirector>().force_begin(0);
            info!("[ui-shots] combat staged: wave 1 running");
        }
        SETTLE => {
            let doomed: Vec<Entity> = {
                let mut q = world.query_filtered::<Entity, With<Enemy>>();
                q.iter(world).collect()
            };
            for e in doomed {
                let _ = world.entity_mut(e).despawn();
            }
            world.resource_mut::<WaveDirector>().force_settle();
            *world.resource_mut::<Selection>() = Selection::None;
            info!("[ui-shots] wave settled; intermission restored");
        }
        EXIT => {
            info!("[ui-shots] done — exiting");
            world.write_message(AppExit::Success);
        }
        _ => {}
    }
    for (frame_at, stem) in SHOTS {
        if f == frame_at {
            let dir = out_dir(state.skin);
            std::fs::create_dir_all(&dir).ok();
            let path = format!("{dir}/{stem}.png");
            info!("[ui-shots] shot -> {path}");
            world
                .spawn(Screenshot::primary_window())
                .observe(save_to_disk(path));
        }
    }
    // The always-on DB3 strip is debug tooling, not game skin — keep it out
    // of the comparison images. Re-checked every frame (it spawns lazily).
    {
        let mut q = world.query_filtered::<Entity, With<crate::systems::debug_time::DebugHudText>>();
        let hits: Vec<Entity> = q.iter(world).collect();
        for e in hits {
            let _ = world.entity_mut(e).insert(Visibility::Hidden);
        }
    }
    world.insert_resource(state);
}

/// Stage the scene: debug mark, a budget with affordable + greyed reads, two
/// archers placed through the player gateway (kit + Fuse pair).
fn stage(world: &mut World, state: &mut UiShotsState) {
    if let Some(mut run) = world.get_resource_mut::<DebugRun>() {
        run.marked = true;
    }
    world.resource_mut::<Economy>().gold = 120;
    let slots: Vec<(Entity, Vec3)> = free_slots_in_order(world).into_iter().take(2).collect();
    if slots.len() < 2 {
        warn!("[ui-shots] fewer than 2 free slots — selected-tower shots degraded");
    }
    if !slots.is_empty() {
        place_towers(world, 0, &slots);
    }
    let towers = {
        let mut q = world.query_filtered::<Entity, With<Tower>>();
        q.iter(world).collect::<Vec<_>>()
    };
    state.hero = towers.first().copied();
    if state.hero.is_none() {
        warn!("[ui-shots] staging placed no tower — card shots fall back to build face");
    }
    info!("[ui-shots] staged: gold 120, {} towers", towers.len());
}

fn select_hero(world: &mut World, state: &mut UiShotsState) {
    if let Some(hero) = state.hero {
        *world.resource_mut::<Selection>() = Selection::Tower(hero);
        info!("[ui-shots] tower selected — card switches to the unit face");
    }
}

/// Which zone stays visible in a solo shot (ui-mock's `ShotMode::Solo`
/// pattern: siblings hidden, the solo panel keeps its layout slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoloTarget {
    Card,
    InfoZone,
}

/// Toggle solo visibility (shared by the `--ui-shots` driver and the
/// offscreen test in `tests/ui_shots.rs`). `Visibility::Hidden` (not
/// `Display::None`) on the zones so the bar layout does not reflow; the
/// field report instead uses `Display::None` because `update_hud` writes
/// its Visibility every frame and would un-hide it in intermission.
pub fn solo_bottom_bar(world: &mut World, target: Option<SoloTarget>) {
    let soloing = target.is_some();
    let (hide_info, hide_card, hide_control) = match target {
        None => (false, false, false),
        Some(SoloTarget::Card) => (true, false, true),
        Some(SoloTarget::InfoZone) => (false, true, true),
    };
    set_marker_hidden::<TopPlaque>(world, soloing);
    {
        let mut q = world.query_filtered::<Entity, With<crate::systems::hud::FieldReport>>();
        let reports: Vec<Entity> = q.iter(world).collect();
        for e in reports {
            // `Display` is a field of `Node` (not a component) — write it
            // in place, the command.rs idiom.
            if let Some(mut node) = world.get_mut::<Node>(e) {
                node.display = if soloing { Display::None } else { Display::Flex };
            }
        }
    }
    set_marker_hidden::<InfoZoneRoot>(world, hide_info);
    set_marker_hidden::<CommandCardRoot>(world, hide_card);
    set_marker_hidden::<ControlZoneRoot>(world, hide_control);
    info!("[ui-shots] solo mode -> {target:?}");
}

fn set_marker_hidden<M: Component>(world: &mut World, hidden: bool) {
    let mut q = world.query_filtered::<Entity, With<M>>();
    let hits: Vec<Entity> = q.iter(world).collect();
    for e in hits {
        let vis = if hidden { Visibility::Hidden } else { Visibility::Visible };
        let _ = world.entity_mut(e).insert(vis);
    }
}

/// Companion to the driver (the `skill_card_cursor_pin` pattern): winit
/// re-reports the absent OS mouse every event batch, clobbering scripted
/// cursor positions — so during the tooltip window re-pin the cursor over
/// the first Build button (physical UI space / window scale = logical).
pub fn ui_shots_cursor_pin(world: &mut World) {
    let Some(state) = world.get_resource::<UiShotsState>() else {
        return;
    };
    if !(PIN_FROM..=PIN_TO).contains(&state.frames) {
        return;
    }
    let target = {
        let mut q = world.query_filtered::<
            (&ComputedNode, &UiGlobalTransform, &CommandButton),
            With<Button>,
        >();
        q.iter(world)
            .find(|(_, _, b)| matches!(b.action, CommandAction::Build(_)))
            .map(|(node, tf, _)| tf.transform_point2(node.size / 2.0))
    };
    let Some(center) = target else {
        return;
    };
    let mut windows = world.query::<&mut Window>();
    if let Ok(mut window) = windows.single_mut(world) {
        let scale = window.scale_factor();
        window.set_cursor_position(Some(center / scale));
    }
}

// ---------------------------------------------------------------------------
// Tests (headless): the script's acceptance contract is data, digitized.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shot frame is inside the script (before EXIT) and strictly
    /// increasing — a scripted run can only take them in order.
    #[test]
    fn shot_frames_are_ordered_and_before_exit() {
        let mut prev = 0;
        for (frame, _) in SHOTS {
            assert!(frame > prev, "shot frames must strictly increase");
            assert!(frame < EXIT, "shots must land before the self-exit");
            prev = frame;
        }
    }

    /// The STAB acceptance is ≥3 images per skin: one full-screen, one
    /// command-card close-up, one info-zone close-up (plus the extra card
    /// states this script throws in for free).
    #[test]
    fn shot_table_covers_the_required_kinds() {
        let stems: Vec<&str> = SHOTS.iter().map(|(_, s)| *s).collect();
        let mut sorted = stems.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), stems.len(), "shot stems must be unique");
        assert!(stems.len() >= 3, "at least 3 shots per skin");
        assert!(
            stems.iter().any(|s| s.contains("bar-build")),
            "a full-screen shot is required"
        );
        assert!(
            stems.iter().any(|s| s.contains("card-closeup")),
            "a command-card close-up is required"
        );
        assert!(
            stems.iter().any(|s| s.contains("info-zone-closeup")),
            "an info-zone close-up is required"
        );
    }

    /// The pin window strictly wraps the tooltip shot: hover must be live
    /// before the capture and held through it.
    #[test]
    fn cursor_pin_window_wraps_the_tooltip_shot() {
        let tooltip = SHOTS.iter().map(|(f, _)| *f).find(|f| *f >= PIN_FROM).expect("tooltip shot");
        assert!(PIN_FROM < tooltip && tooltip < PIN_TO, "pin window must wrap the shot");
    }

    /// Both skins resolve distinct output directories (the A/B pair).
    #[test]
    fn skins_write_to_distinct_dirs() {
        assert_ne!(out_dir(ShotSkin::Default), out_dir(ShotSkin::PlanA));
        assert!(out_dir(ShotSkin::Default).ends_with("/default"));
        assert!(out_dir(ShotSkin::PlanA).ends_with("/plan-a"));
    }
}
