//! Warden's showroom adapter (SM1, ADR-0010): the generic studio lives in
//! `engine/showroom-viewer`; this module only registers WHAT Warden shows —
//! towers (base + fused) and enemies straight from the definition tables,
//! each item's closure assembling through the gameplay appearance seams so
//! the display can never drift from the real game. Entering also freezes
//! Warden's gameplay (GameState::Showroom) and marks the hover ghosts as
//! sweep-kept (their entity ids are cached in a Local).

use bevy::prelude::*;
use showroom_viewer::{Origin, ShowroomCatalog, ShowroomItem, enter_studio};

// Re-exported so the plugin registration and the tests keep one import
// path for the viewer symbols.
pub use showroom_viewer::{
    ShowroomExhibit, ShowroomFloor, ShowroomGalleryItem, ShowroomLight, ShowroomPad, ShowroomPlate,
    ShowroomState,
};

use crate::resources::{EnemyDefs, FusionDefs, Lang, TowerDefs};
use crate::states::GameState;
use crate::systems::enemy::{ENEMY_MODELS, ENEMY_SCALES, enemy_base_color};
use crate::systems::hud::UiFont;
use crate::systems::tower::{
    attach_tower_appearance, fused_tower_color, ARROW_TOWER_MODEL, BALLISTA_MODEL,
};

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
    let manifest = load_manifest();
    let catalog = build_catalog(world, &manifest);
    let font = world.resource::<UiFont>().0.clone();
    world
        .resource_mut::<NextState<GameState>>()
        .set(GameState::Showroom);
    enter_studio(world, catalog, font);
}

// --- SM4: baseline capture sweep ------------------------------------------------------

/// SM4 baseline sweep state (DB2 `Scenario::ShowroomBaseline`): walks every
/// flattened catalog entry, resets the canonical camera pose, waits out the
/// async load, captures one shot per exhibit. `settle` counts down frames
/// between a selection and its capture (GLB loads take ~1s).
#[derive(Resource)]
pub struct ShowroomBaseline {
    idx: usize,
    settle: u32,
    /// Frames left after the last capture before exiting — the screenshot
    /// encodes/writes asynchronously and dies with the app otherwise.
    done: u32,
}

/// Frames to let the staged exhibit settle (async scene load + one present)
/// before the capture. Generous: a wrong frame costs a whole re-run.
const BASELINE_SETTLE_FRAMES: u32 = 90;

/// Post-sweep flush before AppExit (last shot's async disk write).
const BASELINE_DONE_FRAMES: u32 = 60;

/// Enter the studio in baseline mode: the normal showroom, then the sweep
/// resource rides along (the sweep system drives and exits).
pub fn enter_showroom_baseline(world: &mut World) {
    enter_showroom(world);
    // save_to_disk does not create directories; the baseline set is part of
    // the repo (manual-diff material), so make sure the target exists.
    std::fs::create_dir_all("docs/img/showroom").ok();
    world.insert_resource(ShowroomBaseline {
        idx: 0,
        settle: 0,
        done: BASELINE_DONE_FRAMES,
    });
}

/// The SM4 sweep: canonical pose (default orbit, turntable off) per exhibit,
/// one engine-side screenshot each into `docs/img/showroom/`, AppExit when
/// the whole catalog is captured. The egui panel and the nameplate stay in
/// frame on purpose — a baseline carries its own labels. Every viewer
/// resource is optional: the system no-ops outside the studio (and during
/// the frame the scenario is still loading).
pub fn showroom_baseline_sweep(
    mut state: Option<ResMut<ShowroomBaseline>>,
    catalog: Option<Res<ShowroomCatalog>>,
    viewer: Option<ResMut<ShowroomState>>,
    orbit: Option<ResMut<showroom_viewer::ShowroomOrbit>>,
    turntable: Option<ResMut<showroom_viewer::ShowroomTurntable>>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
) {
    let (Some(mut state), Some(catalog), Some(mut viewer), Some(mut orbit), Some(mut turntable)) =
        (state, catalog, viewer, orbit, turntable)
    else {
        return;
    };
    // The flattened catalog cursor: (category, item, name) in browse order.
    let flat: Vec<(usize, usize, String)> = catalog
        .categories
        .iter()
        .enumerate()
        .flat_map(|(ci, c)| {
            c.items
                .iter()
                .enumerate()
                .map(move |(ii, it)| (ci, ii, it.name.clone()))
        })
        .collect();
    if state.idx >= flat.len() {
        // Flush window: the last screenshot still writes to disk.
        if state.done == 0 {
            info!("[sm] baseline sweep done ({} exhibits), exiting", flat.len());
            exit.write(AppExit::Success);
        } else {
            state.done -= 1;
        }
        return;
    }
    if state.settle == 0 {
        // Advance to the next exhibit.
        let Some((ci, ii, _)) = flat.get(state.idx).cloned() else {
            return; // unreachable given the bound check above
        };
        viewer.select(ci, ii);
        *orbit = showroom_viewer::ShowroomOrbit::default(); // canonical pose
        turntable.on = false;
        state.settle = BASELINE_SETTLE_FRAMES;
        return;
    }
    state.settle -= 1;
    if state.settle > 0 {
        return;
    }
    // Capture frame: sanitize the exhibit name into a file path. The
    // category+item indexes disambiguate same-named entries (the shield
    // TOWER and the shield ENEMY are both "shield" in English).
    if let Some((ci, ii, name)) = flat.get(state.idx).cloned() {
        let file = name
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                    ch
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let path = format!("docs/img/showroom/sm_{ci}{ii:02}_{file}.png");
        commands
            .spawn(bevy::render::view::screenshot::Screenshot::primary_window())
            .observe(bevy::render::view::screenshot::save_to_disk(path.clone()));
        info!("[sm] baseline shot ({}/{}) -> {path}", state.idx + 1, flat.len());
    }
    state.idx += 1;
}

/// The Warden catalog: towers (base + fused) then enemies, live-read from
/// the definition tables — new defs join the list with zero changes here.
/// Parsed art-assets.json (ADR-0009): game-dest path -> (id, name, version).
/// Missing file => empty map: origins degrade to GameLocal, never block.
type Manifest = std::collections::HashMap<String, (String, String, String)>;

fn load_manifest() -> Manifest {
    let Ok(raw) = std::fs::read_to_string("art-assets.json") else {
        return Manifest::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Manifest::new();
    };
    let mut m = Manifest::new();
    if let Some(assets) = v.get("assets").and_then(|a| a.as_array()) {
        for a in assets {
            let id = a.get("id").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let name = a.get("name").and_then(|x| x.as_str()).unwrap_or("?").to_string();
            let version = a
                .get("version")
                .and_then(|x| x.as_i64())
                .map(|n| n.to_string())
                .unwrap_or_else(|| "?".into());
            if let Some(files) = a.get("files").and_then(|f| f.as_object()) {
                for dest in files.values() {
                    if let Some(d) = dest.as_str() {
                        m.insert(d.to_string(), (id.clone(), name.clone(), version.clone()));
                    }
                }
            }
        }
    }
    m
}

fn manifest_id(m: &Manifest, path: &str) -> Option<String> {
    m.get(path).map(|(i, _, _)| i.clone())
}

fn manifest_version(m: &Manifest, path: &str) -> Option<String> {
    m.get(path).map(|(_, _, v)| v.clone())
}

fn build_catalog(world: &World, manifest: &Manifest) -> ShowroomCatalog {
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
        let info_rows = vec![
            ("Damage".into(), format!("{:.0}", def.damage)),
            ("Attack speed".into(), format!("{}/s", def.attack_speed)),
            ("Range".into(), format!("{:.0}", def.range)),
            ("Type".into(), format!("{:?}", def.attack_type)),
            ("Cost".into(), format!("{}", def.cost)),
        ];
        let tags = vec![def.name.to_string(), def.label.to_string(), "tower".into()];
        // Catalog closures are 'static — the def must be owned, not borrowed.
        let def = def.clone();
        let is_archer = i == 0;
        builder = builder.item(ShowroomItem {
            name: def.name.to_string(),
            info_rows,
            tags,
            origin: if is_archer {
                vec![
                    Origin::Vault {
                        name: "tower_arrow".into(),
                        id: manifest_id(&manifest, ARROW_TOWER_MODEL).unwrap_or("?".into()),
                        version: manifest_version(&manifest, ARROW_TOWER_MODEL).unwrap_or("?".into()),
                        role: Some("tower body".into()),
                    },
                    Origin::Vault {
                        name: "weapon_ballista".into(),
                        id: manifest_id(&manifest, BALLISTA_MODEL).unwrap_or("?".into()),
                        version: manifest_version(&manifest, BALLISTA_MODEL).unwrap_or("?".into()),
                        role: Some("ballista".into()),
                    },
                ]
            } else {
                vec![Origin::GameLocal { note: Some("gray box".into()) }]
            },
            plate: Some(plate),
            anchor_y: 0.6,
            plate_y: 2.4,
            gltf: None,
            parts: if is_archer {
                vec!["body (tower_arrow)".into(), "ballista (weapon_ballista)".into()]
            } else {
                vec![]
            },
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
        let info_rows = vec![
            ("Damage".into(), format!("{:.0}", def.damage)),
            ("Attack speed".into(), format!("{}/s", def.attack_speed)),
            ("Range".into(), format!("{:.0}", def.range)),
            ("Type".into(), format!("{:?}", def.attack_type)),
            ("AOE".into(), format!("{:.1}", def.aoe_radius)),
            ("Slow".into(), format!("{:.2}/{:.1}s", def.slow_factor, def.slow_duration)),
        ];
        let tags = vec![
            def.name.to_string(),
            def.label.to_string(),
            "tower".into(),
            "fusion".into(),
        ];
        let kind = def.kind;
        builder = builder.item(ShowroomItem {
            name: def.name.to_string(),
            info_rows,
            tags,
            origin: vec![Origin::GameLocal {
                note: Some("game-side fusion (gray box)".into()),
            }],
            plate: Some(plate),
            anchor_y: 0.6,
            plate_y: 2.4,
            gltf: None,
            parts: vec![],
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
        let info_rows = vec![
            ("HP".into(), format!("{:.0}", def.hp)),
            ("Speed".into(), format!("{:.1}", def.speed)),
            ("Armor".into(), format!("{}", def.physical_armor)),
            ("Healer".into(), format!("{}", def.healer)),
            ("Leak".into(), format!("{}", def.leak)),
            ("Kill gold".into(), format!("{}", def.kill_gold)),
        ];
        let tags = vec![def.name.to_string(), def.label.to_string(), "enemy".into()];
        let (item, anchor_y, plate_y) = match server
            .zip(ENEMY_MODELS.get(i).copied())
        {
            Some((server, model)) => {
                let scene = server.load(
                    bevy::gltf::GltfAssetLabel::Scene(0).from_asset(model),
                );
                let gltf = server.load::<bevy::gltf::Gltf>(model);
                let scale = ENEMY_SCALES.get(i).copied().unwrap_or(0.8);
                let model = ENEMY_MODELS.get(i).copied().unwrap_or("");
                let item = ShowroomItem {
                    name: def.name.to_string(),
                    info_rows: info_rows.clone(),
                    tags: tags.clone(),
                    origin: vec![Origin::Vault {
                        name: model.to_string(),
                        id: manifest_id(&manifest, model).unwrap_or("?".into()),
                        version: manifest_version(&manifest, model).unwrap_or("?".into()),
                        role: None,
                    }],
                    plate: Some(plate),
                    anchor_y: 0.0, // feet-origin models sit on the pad
                    plate_y: 2.0,
                    gltf: Some(gltf),
                    parts: vec![],
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
                    info_rows: info_rows.clone(),
                    tags: tags.clone(),
                    origin: vec![Origin::GameLocal { note: Some("gray box".into()) }],
                    plate: Some(plate),
                    anchor_y: 0.4,
                    plate_y: 1.7,
                    gltf: None,
                    parts: vec![],
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
