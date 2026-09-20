//! Warden's showroom adapter (SM1, ADR-0010): the generic studio lives in
//! `engine/showroom-viewer`; this module only registers WHAT Warden shows —
//! towers (base + fused) and enemies straight from the definition tables,
//! each item's closure assembling through the gameplay appearance seams so
//! the display can never drift from the real game. Entering also freezes
//! Warden's gameplay (GameState::Showroom) and marks the hover ghosts as
//! sweep-kept (their entity ids are cached in a Local).

use bevy::prelude::*;
use showroom_viewer::{ShowroomCatalog, ShowroomItem, enter_studio};

// Re-exported so the plugin registration and the tests keep one import
// path for the viewer symbols.
pub use showroom_viewer::{
    ShowroomExhibit, ShowroomFloor, ShowroomLight, ShowroomPad, ShowroomPlate, ShowroomState,
};

use crate::resources::{EnemyDefs, FusionDefs, Lang, TowerDefs};
use crate::states::GameState;
use crate::systems::enemy::{ENEMY_MODELS, ENEMY_SCALES, enemy_base_color};
use crate::systems::hud::UiFont;
use crate::systems::tower::{attach_tower_appearance, fused_tower_color};

fn pick_lang<'a>(zh: &'a str, en: &'a str, lang: &Lang) -> &'a str {
    match *lang {
        Lang::Zh => zh,
        Lang::En => en,
    }
}

/// Enter the studio (DB2 `Scenario::Showroom`): build Warden's catalog,
/// keep+hide the hover ghosts, freeze gameplay, hand over to the generic
/// `enter_studio`. Leaving = restart (dev tool).
pub fn enter_showroom(world: &mut World) {
    // Hover ghosts must survive the sweeps (UI3 caches their entity ids);
    // hide them so they never render in the studio.
    {
        let mut q = world.query_filtered::<Entity, With<crate::systems::pointer::HoverGhost>>();
        let ghosts: Vec<Entity> = q.iter(world).collect();
        for e in ghosts {
            world.entity_mut(e).insert(showroom_viewer::ShowroomKept);
            if let Some(mut vis) = world.get_mut::<Visibility>(e) {
                *vis = Visibility::Hidden;
            }
        }
    }
    let catalog = build_catalog(world);
    let font = world.resource::<UiFont>().0.clone();
    world
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Showroom);
    enter_studio(world, catalog, font);
}

/// The Warden catalog: towers (base + fused) then enemies, live-read from
/// the definition tables — new defs join the list with zero changes here.
fn build_catalog(world: &World) -> ShowroomCatalog {
    let towers = world.resource::<TowerDefs>();
    let fusions = world.resource::<FusionDefs>();
    let enemies = world.resource::<EnemyDefs>();
    let lang = world.resource::<Lang>();
    let cat_label = pick_lang("塔", "Towers", lang);
    let enemy_label = pick_lang("怪", "Enemies", lang);
    let n_towers = towers.list.len() + fusions.list.len();

    let mut builder = ShowroomCatalog::build().category(format!("{cat_label} (+fused)"));
    for (i, def) in towers.list.iter().enumerate() {
        let plate = format!(
            "{} · {} {}/{}",
            pick_lang(def.label, def.label_en, lang),
            cat_label,
            i + 1,
            n_towers
        );
        let info = format!(
            "dmg {:.0} · atk {}/s\nrange {:.0} · {:?}\ncost {}",
            def.damage, def.attack_speed, def.range, def.attack_type, def.cost
        );
        // Catalog closures are 'static — the def must be owned, not borrowed.
        let def = def.clone();
        builder = builder.item(ShowroomItem {
            name: def.name.to_string(),
            info,
            plate: Some(plate),
            anchor_y: 0.6,
            plate_y: 2.4,
            gltf: None,
            spawn: Box::new(
                move |commands: &mut Commands, root, ctx: &mut showroom_viewer::SpawnCtx| {
                    attach_tower_appearance(
                        &mut commands.entity(root),
                        &def,
                        i,
                        ctx.meshes,
                        ctx.materials,
                        ctx.server,
                    );
                },
            ),
        });
    }
    for (j, def) in fusions.list.iter().enumerate() {
        let plate = format!(
            "{} · {} {}/{}",
            pick_lang(def.label, def.label_en, lang),
            cat_label,
            towers.list.len() + j + 1,
            n_towers
        );
        let info = format!(
            "dmg {:.0} · atk {}/s\nrange {:.0} · {:?}\naoe {:.1} · slow {:.2}/{:.1}s",
            def.damage,
            def.attack_speed,
            def.range,
            def.attack_type,
            def.aoe_radius,
            def.slow_factor,
            def.slow_duration
        );
        let kind = def.kind;
        builder = builder.item(ShowroomItem {
            name: def.name.to_string(),
            info,
            plate: Some(plate),
            anchor_y: 0.6,
            plate_y: 2.4,
            gltf: None,
            spawn: Box::new(move |commands: &mut Commands, root, ctx: &mut showroom_viewer::SpawnCtx| {
                let _ = ctx;
                commands.entity(root).insert((
                    Mesh3d(ctx.meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
                    MeshMaterial3d(ctx.materials.add(fused_tower_color(kind))),
                ));
            }),
        });
    }
    let mut builder = builder.finish().category(enemy_label.to_string());
    // ART11 models when an AssetServer exists (feet-origin scene child +
    // gltf handle so the studio's clip browser works); headless keeps the
    // tinted cuboid — same gateway choice as the gameplay spawn path.
    let server = world.get_resource::<AssetServer>();
    for (i, def) in enemies.list.iter().enumerate() {
        let plate = format!(
            "{} · {} {}/{}",
            pick_lang(def.label, def.label_en, lang),
            enemy_label,
            i + 1,
            enemies.list.len()
        );
        let info = format!(
            "hp {:.0} · speed {:.1}\narmor {} · healer {}\nleak {} · gold {}",
            def.hp,
            def.speed,
            def.physical_armor,
            def.healer,
            def.leak,
            def.kill_gold
        );
        let (item, anchor_y, plate_y) = match server
            .zip(ENEMY_MODELS.get(i).copied())
        {
            Some((server, model)) => {
                let scene = server.load(
                    bevy::gltf::GltfAssetLabel::Scene(0).from_asset(model),
                );
                let gltf = server.load::<bevy::gltf::Gltf>(model);
                let scale = ENEMY_SCALES.get(i).copied().unwrap_or(0.8);
                let item = ShowroomItem {
                    name: def.name.to_string(),
                    info,
                    plate: Some(plate),
                    anchor_y: 0.0, // feet-origin models sit on the pad
                    plate_y: 2.0,
                    gltf: Some(gltf),
                    spawn: Box::new(
                        move |commands: &mut Commands, root, _ctx: &mut showroom_viewer::SpawnCtx| {
                            commands.entity(root).with_children(|p| {
                                p.spawn((
                                    bevy::world_serialization::WorldAssetRoot(scene.clone()),
                                    Transform::from_scale(Vec3::splat(scale)),
                                ));
                            });
                        },
                    ),
                };
                (item, 0.0, 2.0)
            }
            None => {
                let color = enemy_base_color(def);
                let item = ShowroomItem {
                    name: def.name.to_string(),
                    info,
                    plate: Some(plate),
                    anchor_y: 0.4,
                    plate_y: 1.7,
                    gltf: None,
                    spawn: Box::new(
                        move |commands: &mut Commands, root, ctx: &mut showroom_viewer::SpawnCtx| {
                            commands.entity(root).insert((
                                Mesh3d(ctx.meshes.add(Cuboid::new(0.7, 0.7, 0.7))),
                                MeshMaterial3d(ctx.materials.add(color)),
                            ));
                        },
                    ),
                };
                (item, 0.4, 1.7)
            }
        };
        let _ = (anchor_y, plate_y); // carried by the item itself
        builder = builder.item(item);
    }
    builder.finish()
}
