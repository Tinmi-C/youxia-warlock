//! Showroom (SM1): one-exhibit-at-a-time browsing for rendered assets —
//! towers (base + fused) and enemies today, anything with a definition table
//! tomorrow. Replaces "boot a real run and wait for a wave" as the art/skill
//! visual-acceptance channel (see the Showroom cards in
//! docs/capability-cards.md).
//!
//! Iron laws (card preamble):
//!   - AUTO-ENUMERATED: the browse list is read from TowerDefs/FusionDefs/
//!     EnemyDefs — a new def joins the cycle with zero showroom changes.
//!   - SAME LOOK AS THE GAME: exhibits go through the very appearance seams
//!     the gameplay paths use (attach_tower_appearance / fused_tower_color /
//!     enemy_base_color) — display and gameplay can never drift.
//!   - ZERO INTERFERENCE: exhibits are pure presentation entities carrying
//!     NO Enemy/Tower/Projectile rule components, so every gameplay query
//!     skips them; waves/economy/base stay untouched while browsing.
//!
//! Keys (starting values): Tab = category, ←/→ = prev/next item. Debug-only
//! like every DB/SM card; entering via a scenario marks the run (save
//! isolation). Logging prefix `[sm]` doubles as the release-scan string.

use bevy::prelude::*;

use crate::resources::{
    DebugPanelCapture, EnemyDefs, FusionDefs, Lang, TowerDefs, TowerDef,
};
use crate::systems::enemy::enemy_base_color;
use crate::systems::hud::UiFont;
use crate::systems::tower::{attach_tower_appearance, fused_tower_color};

/// Where the display pad sits (XZ): open ground right of the first path leg,
/// inside the default camera's view, clear of every tower slot.
const SHOWROOM_PAD: (f32, f32) = (6.0, 5.0);
/// Pad disc radius (world units).
const SHOWROOM_PAD_RADIUS: f32 = 1.7;

/// The browser cursor (inserted by `enter_showroom`, read/driven by
/// `showroom_browse`). Category 0 = towers (base then fused), 1 = enemies.
#[derive(Resource)]
pub struct ShowroomState {
    pub category: usize,
    pub item: usize,
}

/// Marker on the current model exhibit root (there is at most one).
#[derive(Component)]
pub struct ShowroomExhibit;

/// Marker on the exhibit's name-plate child (Text2d, despawns with it).
#[derive(Component)]
pub struct ShowroomPlate;

/// Marker on the persistent display pad disc.
#[derive(Component)]
pub struct ShowroomPad;

/// Number of browseable categories.
const CATEGORY_COUNT: usize = 2;

/// Items in a category — read live from the definition tables, so the count
/// (and the cycle) grows with the defs automatically.
fn category_len(category: usize, towers: &TowerDefs, fusions: &FusionDefs, enemies: &EnemyDefs) -> usize {
    match category {
        0 => towers.list.len() + fusions.list.len(),
        _ => enemies.list.len(),
    }
}

fn pick_lang<'a>(zh: &'a str, en: &'a str, lang: &Lang) -> &'a str {
    match *lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Enter the showroom (DB2 `Scenario::Showroom` → `apply_scenario`): wipe any
/// previous showroom entities, spawn the pad, insert the browser cursor at
/// the first tower. The first exhibit is spawned by `showroom_browse` the
/// same frame (it spawns whenever the state exists and no exhibit does).
pub fn enter_showroom(world: &mut World) {
    {
        let mut q = world.query_filtered::<Entity, With<ShowroomExhibit>>();
        let doomed: Vec<Entity> = q.iter(world).collect();
        for e in doomed {
            world.despawn(e);
        }
    }
    {
        let mut q = world.query_filtered::<Entity, With<ShowroomPad>>();
        let doomed: Vec<Entity> = q.iter(world).collect();
        for e in doomed {
            world.despawn(e);
        }
    }
    let pad_mesh = world
        .resource_mut::<Assets<Mesh>>()
        .add(Circle::new(SHOWROOM_PAD_RADIUS));
    let pad_mat = world
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.17, 0.2),
            perceptual_roughness: 1.0,
            ..default()
        });
    world.spawn((
        ShowroomPad,
        Mesh3d(pad_mesh),
        MeshMaterial3d(pad_mat),
        Transform::from_xyz(SHOWROOM_PAD.0, 0.02, SHOWROOM_PAD.1)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
    ));
    world.insert_resource(ShowroomState {
        category: 0,
        item: 0,
    });
    info!("[sm] showroom entered (Tab = category, arrows = item)");
}

/// The browse loop (SM1): Tab cycles category, ←/→ cycles the item; on any
/// switch (or whenever the state exists without an exhibit — first frame
/// after entering) it spawns the current exhibit through the shared
/// appearance seams. Pure presentation: no rule component is ever attached,
/// so no gameplay system can observe or disturb the exhibits.
#[allow(clippy::too_many_arguments)]
pub fn showroom_browse(
    keys: Res<ButtonInput<KeyCode>>,
    panel: Res<DebugPanelCapture>,
    mut commands: Commands,
    state: Option<ResMut<ShowroomState>>,
    exhibits: Query<Entity, With<ShowroomExhibit>>,
    tower_defs: Res<TowerDefs>,
    fusion_defs: Res<FusionDefs>,
    enemy_defs: Res<EnemyDefs>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    server: Option<Res<AssetServer>>,
    lang: Res<Lang>,
    font: Res<UiFont>,
) {
    let Some(mut state) = state else {
        return;
    };
    // Typing into a debug-panel field must not browse (DB1 capture rule).
    if panel.keyboard {
        return;
    }
    let len =
        |category: usize| category_len(category, &tower_defs, &fusion_defs, &enemy_defs);
    let mut switched = false;
    if keys.just_pressed(KeyCode::Tab) {
        state.category = (state.category + 1) % CATEGORY_COUNT;
        state.item = 0;
        switched = true;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        state.item = (state.item + 1) % len(state.category);
        switched = true;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        state.item = (state.item + len(state.category) - 1) % len(state.category);
        switched = true;
    }
    // Also covers the first frame after `enter_showroom` (no exhibit yet).
    if !switched && !exhibits.is_empty() {
        return;
    }
    for e in &exhibits {
        commands.entity(e).despawn();
    }
    spawn_exhibit(
        &mut commands,
        &state,
        &tower_defs,
        &fusion_defs,
        &enemy_defs,
        &mut meshes,
        &mut materials,
        server.as_deref(),
        &lang,
        &font,
    );
}

/// Spawn the current exhibit on the pad: the model through the same
/// appearance seam the gameplay path uses, plus a bilingual name plate
/// (`{name} · {category} {i}/{n}`) riding it as a child.
#[allow(clippy::too_many_arguments)]
fn spawn_exhibit(
    commands: &mut Commands,
    state: &ShowroomState,
    tower_defs: &TowerDefs,
    fusion_defs: &FusionDefs,
    enemy_defs: &EnemyDefs,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    server: Option<&AssetServer>,
    lang: &Lang,
    font: &UiFont,
) {
    // (label, category label, index, total, anchor Y, plate height)
    let (label, cat, i, n, anchor_y, plate_y) = match state.category {
        0 => {
            let t = tower_defs.list.len();
            if state.item < t {
                let def: &TowerDef = &tower_defs.list[state.item];
                (
                    pick_lang(def.label, def.label_en, lang),
                    pick_lang("塔", "Towers", lang),
                    state.item + 1,
                    t + fusion_defs.list.len(),
                    0.6,
                    2.4,
                )
            } else {
                let def = &fusion_defs.list[state.item - t];
                (
                    pick_lang(def.label, def.label_en, lang),
                    pick_lang("塔", "Towers", lang),
                    state.item + 1,
                    t + fusion_defs.list.len(),
                    0.6,
                    2.4,
                )
            }
        }
        _ => {
            let def = &enemy_defs.list[state.item];
            (
                pick_lang(def.label, def.label_en, lang),
                pick_lang("怪", "Enemies", lang),
                state.item + 1,
                enemy_defs.list.len(),
                0.4,
                1.7,
            )
        }
    };
    let mut ec = commands.spawn((
        ShowroomExhibit,
        Visibility::Visible,
        Transform::from_xyz(SHOWROOM_PAD.0, anchor_y, SHOWROOM_PAD.1),
    ));
    match state.category {
        0 => {
            let t = tower_defs.list.len();
            if state.item < t {
                let def: &TowerDef = &tower_defs.list[state.item];
                attach_tower_appearance(&mut ec, def, state.item, meshes, materials, server);
            } else {
                // Same look as an in-game fused tower: 1.2 box, per-kind color.
                let kind = fusion_defs.list[state.item - t].kind;
                ec.insert((
                    Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
                    MeshMaterial3d(materials.add(fused_tower_color(kind))),
                ));
            }
        }
        _ => {
            let def = &enemy_defs.list[state.item];
            ec.insert((
                Mesh3d(meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
                MeshMaterial3d(materials.add(enemy_base_color(def))),
            ));
        }
    }
    let text = format!("{label} · {cat} {i}/{n}");
    info!("[sm] showing {text}");
    ec.with_children(|p| {
        p.spawn((
            ShowroomPlate,
            Text2d::new(text),
            TextFont {
                font: font.0.clone(),
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(Color::srgb(1.0, 0.92, 0.55)),
            // Tilt toward the 3/4 camera so the plate reads upright (DB3 idiom).
            Transform::from_xyz(0.0, plate_y, 0.0)
                .with_rotation(Quat::from_rotation_x(-0.67)),
        ));
    });
}
