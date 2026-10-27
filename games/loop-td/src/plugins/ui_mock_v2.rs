//! `--ui-mock v2` (LT2-UI acceptance): stages the six LT2 features' showcase
//! states on REAL game state and screenshots the REAL Plan-A panels
//! (systems/panels.rs) — nothing here re-renders a panel, it only authors
//! the world the panels read. Runs the real game (map, towers, combat
//! pipeline); shots land in <repo>/ui-proposals/lt2-final/.
//!
//! Staged states: hero equipped (blade12 + scope, one slot free), artifact
//! quest complete-then-claimed (gold plaque + team aura rings), challenge
//! offer at the wave-11 milestone intermission, dig stats with a gold
//! outcome, the arrow_ice bond satisfied (cannon_sentry half-placed), wave
//! running with a live squad for the combat panorama.
//!
//! Frame script (60fps): build panorama -> combat panorama -> per-panel solo
//! close-ups -> self-exit. Design-proposal tooling, not shipping gameplay.

use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use warbase::prelude::*;
use warbase::ui::{assign_grid_hotkeys, CommandAction, CommandCardDef, CommandCards, CommandSlotDef, Hotkey};

use crate::systems::gameplay::{try_place, BuildCursor};
use crate::systems::panels::PanelKind;

#[derive(Resource, Debug)]
struct MockFrame2(u64);

#[derive(Resource, Debug)]
struct MockHero2(Entity);

pub struct UiMockV2Plugin;

impl Plugin for UiMockV2Plugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MockFrame2(0))
            .add_systems(
                Startup,
                stage_v2_scene.after(crate::systems::gameplay::load_map),
            )
            .add_systems(Update, v2_frame_driver);
    }
}

fn out_dir() -> String {
    // <crate>/../../ui-proposals/lt2-final — the worktree root no matter the
    // directory cargo run was started from.
    format!("{}/../../ui-proposals/lt2-final", env!("CARGO_MANIFEST_DIR"))
}

/// Stage the showcase world: econ, milestone intermission, bond-satisfying
/// towers, the hero (XP/HP/mana authored), the claimable artifact quest, dig
/// stats, and the hunter command card. Equipment rides the real equip path
/// on frame 6 (the rack attaches on the first Update after spawn).
fn stage_v2_scene(world: &mut World) {
    // Fund placements first, author the display numbers after.
    world.resource_mut::<Resources>().gold = 20_000;
    world.resource_mut::<Population>().cap = 40;
    world.resource_mut::<StockCount>().0 = 38;

    // Director parked at the wave-11 milestone intermission: the challenge
    // offer is live, 10 waves cleared (quest-consistent).
    {
        let mut d = world.resource_mut::<WaveDirector>();
        d.next_wave = 10;
        d.phase = DirectorPhase::Intermission;
        d.phase_timer = 12.0;
    }

    // Towers: 2 arrow + 1 ice satisfies 箭雨急冻; 1 cannon leaves 机动哨卫
    // half-placed (inactive chip with live counts). Grid cells are world
    // +(14, 9): loop.ron is 28x18. Cells ON the ring rows (world y = ±6)
    // are path-blocked — keep off them (the f48 log pinned this once).
    let towers: &[(&str, i32, i32)] = &[
        ("arrow", 8, 4), ("arrow", 9, 2), ("ice", 6, 7), ("cannon", 8, -7),
    ];
    for (def, wx, wy) in towers {
        if let Err(e) = try_place(world, def, Cell(*wx + 14, *wy + 9)) {
            info!("[ui-mock-v2] place {def} at world ({wx},{wy}) failed: {e}");
        }
    }

    // Hero: level 7, damaged, mana for the bars.
    let hero = try_place(world, "hunter", Cell(3 + 14, 4 + 9)).expect("place hunter");
    if let Some(mut xp) = world.get_mut::<XpHolder>(hero) {
        xp.level = 7;
        xp.talents_taken = 1;
        xp.xp = 2140;
        xp.next_level_xp = 5300;
    }
    if let Some(mut hp) = world.get_mut::<Hp>(hero) {
        hp.0 = 96.0;
    }
    let _ = world.entity_mut(hero).insert((Mana(38.0), MaxMana(60.0)));
    world.insert_resource(MockHero2(hero));

    // Display econ + cursor (cell 20,5 = world 6.5,-3.5).
    world.resource_mut::<Resources>().gold = 347;
    world.resource_mut::<Population>().cap = 20;
    world.resource_mut::<BuildCursor>().0 = Cell(20, 5);

    // Artifact quest complete (claimable — the [Q] plaque state).
    world.insert_resource(crate::systems::artifact::ArtifactQuest {
        waves_cleared: crate::systems::artifact::ARTIFACT_WAVES_REQUIRED,
        boss_kills: crate::systems::artifact::ARTIFACT_BOSSES_REQUIRED,
        claimed: false,
    });

    // Dig stats: 3 rolls, one +80 gold, one equipment (hero bag).
    world.insert_resource(crate::systems::dig::DigStats {
        rolls: 3,
        gold_won: crate::systems::dig::DIG_GOLD_AMOUNT,
        equipment_found: 1,
        last: Some(crate::systems::dig::DigOutcome::Gold(
            crate::systems::dig::DIG_GOLD_AMOUNT,
        )),
    });

    // Hunter command card (the console face while the hero is selected).
    let mut card = CommandCardDef {
        slots: vec![
            mock_slot("Snipe", "autocast / 60 dmg", CommandAction::Cast(0), true),
            mock_slot("Frost", "Frost Arrow / mana 40 / cd 6s", CommandAction::Cast(1), false),
            mock_slot("Upgrade", "90g / tier up", CommandAction::Upgrade, true),
            mock_slot("Sell", "refund 70%", CommandAction::Sell, true),
        ],
    };
    assign_grid_hotkeys(&mut card);
    world
        .resource_mut::<CommandCards>()
        .cards
        .insert("hunter".to_string(), card);

    info!("[ui-mock-v2] staged: milestone intermission, hero + bonds on field");
}

fn mock_slot(label: &str, sub: &str, action: CommandAction, enabled: bool) -> CommandSlotDef {
    CommandSlotDef {
        label: label.into(),
        hotkey: Hotkey::None,
        tooltip: sub.into(),
        action,
        enabled,
        badge: None,
    }
}

/// Frame 6 (after attach_equip_slots ran): equip through the REAL path.
fn stage_equips(world: &mut World) {
    let hero = world.resource::<MockHero2>().0;
    for (slot, item) in [(0, "blade12"), (1, "scope")] {
        if let Err(e) = crate::systems::equip::equip_item(world, hero, slot, item) {
            info!("[ui-mock-v2] equip {item} failed: {e}");
        }
    }
    info!("[ui-mock-v2] hero equipped: blade12 + scope (slot 3 free)");
}

/// A staged squad around the ring so the combat panorama reads.
fn stage_squad(world: &mut World) {
    let path_e = world.resource::<Paths>().0[0];
    let registry = world.resource::<UnitRegistry>().clone();
    let squad: &[(&str, f32, f32)] = &[
        ("grunt", 2.0, 0.8), ("grunt", 4.0, 0.55), ("grunt", 6.0, 1.0),
        ("fast", 9.0, 0.7), ("grunt", 12.0, 0.9), ("tank", 18.0, 0.65),
        ("flyer", 24.0, 1.0), ("grunt", 30.0, 0.45), ("fast", 34.0, 0.85),
    ];
    for (def, progress, hp_frac) in squad {
        let pos = world
            .get::<PathDef>(path_e)
            .map(|p| p.sample(*progress))
            .unwrap_or_default();
        if let Some(u) = spawn_unit(
            world,
            &registry,
            def,
            Vec3::new(pos.x, 0.0, pos.y),
            Allegiance::Attacker,
            4.0,
        ) {
            let _ = world.entity_mut(u).insert((
                PathFollower { path: path_e, progress: *progress, laps: 0, finished: false },
                WaveTag::Normal(10),
            ));
            if *hp_frac < 1.0 {
                if let Some(max) = world.get::<MaxHp>(u).map(|m| m.0) {
                    if let Some(mut hp) = world.get_mut::<Hp>(u) {
                        hp.0 = max * hp_frac;
                    }
                }
            }
        }
    }
}

/// Strip the aura rings + tower markers (back to the claimable state).
fn strip_rings(world: &mut World) {
    let rings: Vec<Entity> = world
        .query_filtered::<Entity, With<crate::systems::panels::AuraRing>>()
        .iter(world)
        .collect();
    for e in rings {
        let _ = world.entity_mut(e).despawn();
    }
    let marked: Vec<Entity> = world
        .query_filtered::<Entity, With<crate::systems::panels::HasAuraRing>>()
        .iter(world)
        .collect();
    for e in marked {
        let _ = world.entity_mut(e).remove::<crate::systems::panels::HasAuraRing>();
    }
}

fn select_hero(world: &mut World) {
    let hero = world.resource::<MockHero2>().0;
    world.insert_resource(warbase::ui::Selection {
        mode: warbase::ui::SelectionMode::Unit(hero),
    });
}

fn shot(world: &mut World, stem: &str) {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).ok();
    let path = format!("{dir}/{stem}.png");
    info!("[ui-mock-v2] shot -> {path}");
    world.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
}

fn v2_frame_driver(world: &mut World) {
    let frame = &mut world.resource_mut::<MockFrame2>().0;
    *frame += 1;
    let f = *frame;

    if f == 6 {
        stage_equips(world);
    }

    match f {
        // 01: build-mode panorama (all panels, challenge offered, artifact
        // claimable, dig live, bond one active).
        24 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(None));
            world.insert_resource(warbase::ui::Selection {
                mode: warbase::ui::SelectionMode::Build,
            });
        }
        36 => shot(world, "01-panorama-build"),
        // 02: combat panorama — challenge armed, wave 11 running, artifact
        // claimed (rings out), hero selected.
        48 => {
            if let Err(e) = crate::systems::challenge::arm_challenge(world) {
                info!("[ui-mock-v2] arm: {e}");
            }
            world.resource_mut::<WaveDirector>().force_begin(10);
            if let Err(e) = crate::systems::artifact::claim_artifact(world) {
                info!("[ui-mock-v2] claim: {e}");
            }
            select_hero(world);
            stage_squad(world);
        }
        60 => shot(world, "02-panorama-combat"),
        // 03: equip rack close-up (2 filled + 1 empty).
        72 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Equip)));
            select_hero(world);
        }
        84 => shot(world, "03-equip-bar"),
        // 04: artifact plaque back to claimable (gold highlight).
        96 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Artifact)));
            crate::systems::artifact::reset(world);
            strip_rings(world);
            world.insert_resource(crate::systems::artifact::ArtifactQuest {
                waves_cleared: crate::systems::artifact::ARTIFACT_WAVES_REQUIRED,
                boss_kills: crate::systems::artifact::ARTIFACT_BOSSES_REQUIRED,
                claimed: false,
            });
        }
        108 => shot(world, "04-artifact-claimable"),
        // 05: claimed (rings re-applied on the next panel pass).
        120 => {
            if let Err(e) = crate::systems::artifact::claim_artifact(world) {
                info!("[ui-mock-v2] re-claim: {e}");
            }
        }
        132 => shot(world, "05-artifact-claimed"),
        // 06: challenge offer (fresh milestone intermission).
        144 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Challenge)));
            world.remove_resource::<crate::systems::challenge::ChallengeState>();
            {
                let mut d = world.resource_mut::<WaveDirector>();
                d.next_wave = 10;
                d.phase = DirectorPhase::Intermission;
                d.phase_timer = 12.0;
            }
        }
        156 => shot(world, "06-challenge-entry"),
        // 07: dig entry (intermission, affordable, last outcome +80金).
        168 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Dig)));
        }
        180 => shot(world, "07-dig-entry"),
        // 08: bond chips (one active, one half-placed).
        192 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Bonds)));
        }
        204 => shot(world, "08-bond-chips"),
        // 09: hero panel (bars + affix lines).
        216 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Hero)));
            select_hero(world);
        }
        228 => shot(world, "09-hero-panel"),
        // 10: command console (hero card face).
        240 => {
            world.insert_resource(crate::systems::panels::PanelsSolo(Some(PanelKind::Command)));
            select_hero(world);
        }
        252 => shot(world, "10-command-console"),
        _ => {}
    }

    if f >= 268 {
        info!("[ui-mock-v2] done — exiting");
        world.write_message(AppExit::Success);
    }
}
