//! FXP acceptance (round-3 plan Phase 1): damage floats (DamageDealt ->
//! pooled floating numbers with crit tiering), status markers (Stunned/
//! Silenced diamonds + aura rings), kill bursts (UnitDied -> 0.3s pulse).
//! Headless — logic data and component styling only; the visual pass is the
//! fx-preview screenshot set (ui-proposals/fx-preview/).

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use warbase::fx::{
    float_style, marker_style, AuraRing, DamageFloat, KillBurst, StatusMarker,
    DAMAGE_FLOAT_POOL, DAMAGE_FLOAT_TTL, KILL_BURST_SECS,
};
use warbase::prelude::*;
use warbase::testing::test_app;

/// A bare unit: just a position (everything FXP reads is optional-tolerant).
fn unit(app: &mut App, pos: Vec3) -> Entity {
    app.world_mut().spawn(Transform::from_translation(pos)).id()
}

/// Owned snapshots (no world borrows held across updates).
fn floats(app: &mut App) -> Vec<(Entity, DamageFloat)> {
    let mut q = app.world_mut().query_filtered::<(Entity, &DamageFloat), ()>();
    q.iter(app.world()).map(|(e, f)| (e, f.clone())).collect()
}

fn write_dealt(app: &mut App, target: Entity, amount: f32, crit: bool) {
    app.world_mut()
        .resource_mut::<Messages<DamageDealt>>()
        .write(DamageDealt {
            target,
            source: DamageOrigin::System,
            amount,
            crit,
            killed: false,
        });
}

/// AC1: one DamageDealt => exactly one DamageFloat; the number is rounded
/// and the position captured from the target's transform.
#[test]
fn one_damage_dealt_one_float_rounded_and_positioned() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::new(3.0, 0.0, 5.0));
    write_dealt(&mut app, u, 12.6, false);
    app.update();
    let shown = floats(&mut app);
    assert_eq!(shown.len(), 1, "exactly one float per DamageDealt");
    let f = &shown[0].1;
    assert_eq!(f.text, "13", "amount rounded to the nearest integer");
    assert_eq!(f.world_pos, Vec3::new(3.0, 0.0, 5.0), "position captured");
    assert_eq!(f.ttl, DAMAGE_FLOAT_TTL);
    assert!(!f.crit);

    // Bare entity (no Transform): still exactly one float, ZERO position —
    // a hit must never vanish for a missing component.
    let bare = app.world_mut().spawn(()).id();
    write_dealt(&mut app, bare, 7.0, false);
    app.update();
    assert_eq!(floats(&mut app).len(), 2);
    let last = floats(&mut app).into_iter().find(|(e, _)| *e != shown[0].0).unwrap().1;
    assert_eq!(last.world_pos, Vec3::ZERO);
}

/// AC1b: floats age out and despawn at ttl (~1.2s at 1/60).
#[test]
fn floats_expire_at_ttl() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::ZERO);
    write_dealt(&mut app, u, 10.0, false);
    app.update();
    assert_eq!(floats(&mut app).len(), 1);
    for _ in 0..75 {
        app.update(); // 75 frames = 1.25s > 1.2s ttl
    }
    assert!(floats(&mut app).is_empty(), "float despawned past ttl");
}

/// AC2: the crit tier is distinguished by color AND font size, both on the
/// data-driven style helper and on the actually-rendered text components.
#[test]
fn crit_tier_color_and_size() {
    let mut app = test_app();
    app.update();
    let a = unit(&mut app, Vec3::ZERO);
    let b = unit(&mut app, Vec3::new(1.0, 0.0, 0.0));
    write_dealt(&mut app, a, 20.0, false);
    write_dealt(&mut app, b, 99.0, true);
    app.update();
    app.update(); // render decoration (Text bundle) lands via commands

    let (normal_color, normal_size) = float_style(false);
    let (crit_color, crit_size) = float_style(true);
    assert_ne!(normal_color.to_srgba(), crit_color.to_srgba(), "tiers differ in color");
    assert!(crit_size > normal_size, "crit is the larger tier");

    let mut q = app
        .world_mut()
        .query_filtered::<(&DamageFloat, &TextFont, &TextColor), ()>();
    let mut saw_normal = false;
    let mut saw_crit = false;
    for (f, font, color) in q.iter(app.world()) {
        let (want_color, want_size) = float_style(f.crit);
        let got_size = match font.font_size {
            FontSize::Px(v) => v,
            _ => panic!("expected pixel font size"),
        };
        assert_eq!(got_size, want_size, "spawned size matches the tier");
        assert_eq!(color.0.to_srgba(), want_color.to_srgba(), "spawned color matches the tier");
        if f.crit {
            saw_crit = true;
        } else {
            saw_normal = true;
        }
    }
    assert!(saw_normal && saw_crit, "both tiers spawned text");
}

/// AC3: 33 hits in one frame leave exactly 32 floats — the pool drops the
/// OLDEST (the first of the same-frame tie, by spawn order).
#[test]
fn pool_caps_at_32_dropping_oldest() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::ZERO);
    for i in 1..=33i64 {
        write_dealt(&mut app, u, i as f32, false);
    }
    app.update();
    let shown = floats(&mut app);
    assert_eq!(shown.len(), DAMAGE_FLOAT_POOL, "pool capped at 32");
    let texts: Vec<&str> = shown.iter().map(|(_, f)| f.text.as_str()).collect();
    assert!(!texts.contains(&"1"), "the oldest float (amount 1) was dropped");
    assert!(texts.contains(&"33"), "the newest float survived");
    // Steady state: one more hit next frame still keeps the pool at 32.
    write_dealt(&mut app, u, 34.0, false);
    app.update();
    assert_eq!(floats(&mut app).len(), DAMAGE_FLOAT_POOL);
    let all = floats(&mut app);
    let texts: Vec<&str> = all.iter().map(|(_, f)| f.text.as_str()).collect();
    assert!(!texts.contains(&"2"), "next-oldest dropped on overflow");
}

/// AC4: a stun/silence landing produces its StatusMarker within one frame;
/// removing the status removes the marker; both statuses coexist as two
/// markers on one unit.
#[test]
fn status_markers_appear_within_a_frame_and_vanish() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::new(1.0, 0.0, 2.0));
    app.world_mut().entity_mut(u).insert(Stunned { remaining: 5.0 });
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        let markers: Vec<&StatusMarker> = q.iter(app.world()).collect();
        assert_eq!(markers.len(), 1, "stun marker within one frame");
        assert_eq!(markers[0].target, u);
        assert_eq!(markers[0].kind, StatusKind::Stun);
    }

    // Silence on the same unit: two markers side by side.
    app.world_mut().entity_mut(u).insert(Silenced { remaining: 5.0 });
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        assert_eq!(q.iter(app.world()).count(), 2, "stun + silence coexist");
    }

    // Status removed => marker removed the same frame.
    app.world_mut().entity_mut(u).remove::<Stunned>();
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        let markers: Vec<&StatusMarker> = q.iter(app.world()).collect();
        assert_eq!(markers.len(), 1, "stun marker dropped with the status");
        assert_eq!(markers[0].kind, StatusKind::Silence);
    }

    // The marker glyphs/styles are the pinned CJK pair.
    let (stun_color, stun_glyph) = marker_style(StatusKind::Stun);
    let (sil_color, sil_glyph) = marker_style(StatusKind::Silence);
    assert_eq!(stun_glyph, "晕");
    assert_eq!(sil_glyph, "静");
    assert_ne!(stun_color.to_srgba(), sil_color.to_srgba());

    // Natural expiry (timer ticked to zero by the skill Tick phase) also
    // drops the marker.
    app.world_mut()
        .resource_mut::<Messages<ApplyStatus>>()
        .write(ApplyStatus { target: u, status: StatusKind::Stun, duration: 0.02 });
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        assert_eq!(q.iter(app.world()).count(), 2, "refresh re-adds the stun marker");
    }
    for _ in 0..4 {
        app.update(); // 0.02s stun expires, silence (5s) holds
    }
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        let markers: Vec<&StatusMarker> = q.iter(app.world()).collect();
        assert_eq!(markers.len(), 1, "expired stun removed its marker");
        assert_eq!(markers[0].kind, StatusKind::Silence);
    }

    // Target despawned => marker gone.
    let _ = app.world_mut().despawn(u);
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&StatusMarker, ()>();
        assert!(q.iter(app.world()).next().is_none(), "despawned target drops its marker");
    }
}

/// AC4b: aura carriers get one thin ground ring child; losing the Auras
/// component removes it.
#[test]
fn aura_rings_track_carriers() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::ZERO);
    app.world_mut().entity_mut(u).insert(Auras(vec![AuraDef {
        radius: 1.2,
        kind: StatKind::DamageMult,
        amount: 0.1,
        affects: AuraAffects::Allies,
    }]));
    app.update();
    app.update(); // ring spawns via deferred child command
    let ring = {
        let mut q = app.world_mut().query_filtered::<&AuraRing, ()>();
        q.iter(app.world()).find(|r| r.target == u).copied()
    };
    assert!(ring.is_some(), "carrier has an aura ring");
    // Ring rides as a child of the unit (follows movement, dies with it).
    {
        let world = app.world();
        let kids: Vec<Entity> = world
            .get::<Children>(u)
            .expect("ring is a child")
            .iter()
            .collect();
        assert!(kids.iter().any(|c| world.get::<AuraRing>(*c).is_some()));
    }
    app.world_mut().entity_mut(u).remove::<Auras>();
    app.update();
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&AuraRing, ()>();
        assert!(q.iter(app.world()).next().is_none(), "ring removed with the auras");
    }
}

/// AC5: one UnitDied => one KillBurst at the body's last position, gone
/// after KILL_BURST_SECS; a dead-gone body (despawned before the FX pass)
/// is skipped silently.
#[test]
fn kill_burst_pulses_and_despawns() {
    let mut app = test_app();
    app.update();
    let u = unit(&mut app, Vec3::new(2.0, 0.0, -1.0));
    app.world_mut()
        .resource_mut::<Messages<UnitDied>>()
        .write(UnitDied {
            entity: u,
            def: "grunt".into(),
            gold: 0,
            wood: 0,
            xp: 0,
            killer: Some(DamageOrigin::System),
        });
    app.update();
    app.update(); // spawn lands via commands
    {
        let mut q = app.world_mut().query_filtered::<&KillBurst, ()>();
        let bursts: Vec<&KillBurst> = q.iter(app.world()).collect();
        assert_eq!(bursts.len(), 1, "one burst per death");
        // The spawn lands via commands and a frame or two have since ticked
        // it: a fresh burst is within a couple of STEPs of full charge.
        assert!(
            bursts[0].timer <= KILL_BURST_SECS
                && bursts[0].timer > KILL_BURST_SECS - 2.5 * warbase::testing::STEP as f32,
            "fresh burst timer ~{} (got {})",
            KILL_BURST_SECS,
            bursts[0].timer
        );
    }
    let at = {
        let mut q = app.world_mut().query_filtered::<&Transform, With<KillBurst>>();
        q.single(app.world()).unwrap().translation
    };
    assert_eq!((at.x, at.z), (2.0, -1.0), "burst at the body's position");

    for _ in 0..20 {
        app.update(); // 20 frames = 0.33s > 0.3s
    }
    {
        let mut q = app.world_mut().query_filtered::<&KillBurst, ()>();
        assert!(q.iter(app.world()).next().is_none(), "burst despawned after 0.3s");
    }

    // Body already despawned: skipped silently (no panic, no burst).
    let ghost = app.world_mut().spawn(()).id();
    let _ = app.world_mut().despawn(ghost);
    app.world_mut()
        .resource_mut::<Messages<UnitDied>>()
        .write(UnitDied {
            entity: ghost,
            def: "ghost".into(),
            gold: 0,
            wood: 0,
            xp: 0,
            killer: None,
        });
    app.update();
    app.update();
    {
        let mut q = app.world_mut().query_filtered::<&KillBurst, ()>();
        assert!(q.iter(app.world()).next().is_none(), "gone body => no burst");
    }
}
