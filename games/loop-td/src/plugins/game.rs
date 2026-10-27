//! GamePlugin: core (map load + input + HUD — headless-safe) and visuals
//! (real game only). One domain = one plugin.

use bevy::prelude::*;

use crate::systems::{gameplay, hud, visuals};

/// Headless-safe core: loads the map, drives build input, refreshes HUD.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // LT2-UI: the boarded Plan-A "Iron & Gold" skin — every panel (these
        // and the warbase command surfaces) reads tokens from it; swapping
        // the resource/RON re-skins the whole HUD with zero code change.
        app.insert_resource(crate::systems::theme::plan_a());
        // hud::refresh_hud takes Res<SelectedTower>; nothing else inserts it
        // (BX4b card-switch leftover) — windowed runs panicked at startup.
    app.insert_resource(gameplay::SelectedTower(String::new()))
            .add_plugins(crate::systems::panels::PanelsPlugin)
            .add_systems(Startup, gameplay::load_map)
            .add_systems(Startup, gameplay::load_meta_startup)
            .add_systems(Startup, hud::spawn_hud)
            // LT-ADOPT: digit-row modal yield runs before the base input
            // bridges (Update) so the claim is always settled for them; the
            // TD selection kit lands on fresh spawns (Selectable towers,
            // hero Inventory) for the base mouse chain.
            .add_systems(PreUpdate, crate::systems::adopt::sync_modal_claim)
            .add_systems(Update, crate::systems::adopt::attach_td_kit)
            // LT2-1: hero towers carry a 3-slot equipment rack (E equips
            // from the bag at the build cursor).
            .add_systems(Update, (crate::systems::equip::attach_equip_slots, crate::systems::equip::equip_input))
            // LT2-2: artifact quest progress + team-wide aura (the claim
            // rides the build card's Custom "artifact" slot, SEAM3b).
            .add_systems(
                Update,
                (
                    crate::systems::artifact::track_artifact_progress,
                    crate::systems::artifact::apply_artifact_aura,
                ),
            )
            // LT2-3: challenge-wave restore + drop (arming rides the build
            // card's Custom "challenge" slot; key C preserved as its hotkey).
            .add_systems(
                Update,
                crate::systems::challenge::challenge_progress,
            )
            .add_systems(Update, gameplay::build_input)
            // LT2-5: tower-combo bonds re-resolve every frame.
            .add_systems(Update, crate::systems::bonds::apply_bonds)
            // LT2-6: multiplayer leak pass moved to the base rule domain
            // (SEAM3b PassToNeighbor uplift) — runs inside WarbaseSet.
            // LT2-4: treasure digs ride the build card's Custom "dig" slot
            // (key V preserved as its hotkey).
            .add_systems(Update, gameplay::command_dispatch)
            .add_systems(Update, gameplay::snipe_effect)
            .add_systems(Update, gameplay::hero_talents)
            .add_systems(Update, (gameplay::bank_run, gameplay::end_screen_input))
            .add_systems(Update, (gameplay::track_stats, gameplay::hero_acquisition, gameplay::refresh_end_screen))
            .add_systems(Update, hud::refresh_hud);
    }
}

/// Greybox visuals: camera, ground, path ring, unit meshes, cursor.
/// Never added in headless tests (needs render asset stores).
pub struct VisualPlugin;

impl Plugin for VisualPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (crate::systems::camera::spawn_camera, visuals::spawn_scene),
        )
        .add_systems(Update, (visuals::move_cursor_highlight, visuals::visualize_units));
    }
}
