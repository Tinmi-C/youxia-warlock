//! WX4 acceptance: the generic battle FX — health bars (with game styling),
//! hit flash, blast rings, death shells. Headless (components only; the
//! visual pass is the H3 human gate).

use bevy::prelude::*;
use warbase::fx::{
    spawn_death_shell, BlastRing, DeathShell, FxBlast, HealthBar, HealthBarStyle, HitFlash,
    BAR_COLOR,
};
use warbase::prelude::*;
use warbase::testing::test_app;

/// A bare visual unit: Hp/MaxHp/Transform + its own material handle.
fn visual_unit(app: &mut App, hp: f32, max: f32, style: Option<HealthBarStyle>) -> Entity {
    let e = app
        .world_mut()
        .spawn((
            Hp(hp),
            MaxHp(max),
            Transform::from_xyz(0.0, 0.4, 0.0),
        ))
        .id();
    // Give it a real material handle (the flash/bar systems mutate assets).
    {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let handle = materials.add(StandardMaterial {
            base_color: Color::srgb(0.8, 0.2, 0.2),
            ..default()
        });
        app.world_mut().entity_mut(e).insert(MeshMaterial3d(handle));
    }
    if let Some(style) = style {
        app.world_mut().entity_mut(e).insert(style);
    }
    e
}

fn material_color(app: &mut App, e: Entity) -> Color {
    let handle = app
        .world()
        .get::<MeshMaterial3d<StandardMaterial>>(e)
        .expect("material")
        .0
        .clone();
    app.world()
        .resource::<Assets<StandardMaterial>>()
        .get(&handle)
        .expect("asset")
        .base_color
}

fn bars(app: &mut App) -> Vec<(Entity, f32, bool)> {
    let mut q = app
        .world_mut()
        .query_filtered::<(&HealthBar, &Transform, &Visibility), ()>();
    q.iter(app.world())
        .map(|(b, tf, v)| (b.target, tf.scale.x, *v == Visibility::Visible))
        .collect()
}

/// WX4 AC1: no bar at full hp; damaged => bar with width == ratio x width;
/// healed back to full => hidden. A HealthBarStyle overrides width + color.
#[test]
fn health_bar_visibility_ratio_and_style() {
    let mut app = test_app();
    let plain = visual_unit(&mut app, 30.0, 30.0, None);
    let elite = visual_unit(
        &mut app,
        60.0,
        60.0,
        Some(HealthBarStyle { width: 1.4, color: Color::srgb(1.0, 0.8, 0.25) }),
    );
    app.update();
    assert!(bars(&mut app).is_empty(), "full-health units have no bar");

    app.world_mut().get_mut::<Hp>(plain).unwrap().0 = 15.0;
    app.world_mut().get_mut::<Hp>(elite).unwrap().0 = 30.0;
    app.update();
    app.update(); // bars spawn via commands
    let shown = bars(&mut app);
    assert_eq!(shown.len(), 2, "both damaged units get bars");
    assert!(shown.iter().all(|(_, _, v)| *v), "bars visible");
    // ratio 0.5: plain => 0.5 x 0.9, elite => 0.5 x 1.4 (by target — query
    // order is not stable).
    let width_of = |target: Entity| {
        shown
            .iter()
            .find(|(t, _, _)| *t == target)
            .map(|(_, w, _)| *w)
            .expect("bar for the unit")
    };
    assert!((width_of(plain) - 0.45).abs() < 0.01, "plain width (got {})", width_of(plain));
    assert!((width_of(elite) - 0.70).abs() < 0.01, "styled width (got {})", width_of(elite));

    // Styled bar carries its color; plain bar the default.
    let colors: Vec<bevy::color::Srgba> = {
        let mut q = app
            .world_mut()
            .query_filtered::<(&HealthBar, &MeshMaterial3d<StandardMaterial>), ()>();
        let world = app.world_mut();
        q.iter(world)
            .map(|(b, m)| {
                let _ = b;
                world
                    .resource::<Assets<StandardMaterial>>()
                    .get(&m.0)
                    .unwrap()
                    .base_color
                    .to_srgba()
            })
            .collect()
    };
    assert!(
        colors.iter().any(|c| *c == BAR_COLOR.to_srgba()),
        "default bar color present"
    );
    assert!(
        colors.iter().any(|c| *c == Color::srgb(1.0, 0.8, 0.25).to_srgba()),
        "styled bar color present"
    );

    app.world_mut().get_mut::<Hp>(plain).unwrap().0 = 30.0;
    app.update();
    let plain_visible = {
        let mut q = app
            .world_mut()
            .query_filtered::<(&HealthBar, &Visibility), ()>();
        q.iter(app.world())
            .find(|(b, _)| b.target == plain)
            .map(|(_, v)| *v == Visibility::Visible)
            .unwrap()
    };
    assert!(!plain_visible, "healed back to full => hidden");
}

/// WX4 AC2: a hit flashes white and restores the original color within
/// HIT_FLASH_SECS, removing the component.
#[test]
fn hit_flash_expires_and_restores() {
    let mut app = test_app();
    let e = visual_unit(&mut app, 30.0, 30.0, None);
    app.update();
    let before = material_color(&mut app, e);

    app.world_mut()
        .entity_mut(e)
        .insert(HitFlash { timer: 0.15, original: None });
    app.update();
    assert!(material_color(&mut app, e).to_srgba().red > 0.9, "flashed white");

    for _ in 0..10 {
        app.update(); // ~0.167s > 0.15s window
    }
    assert!(app.world().get::<HitFlash>(e).is_none(), "component expires");
    assert_eq!(
        material_color(&mut app, e).to_srgba(),
        before.to_srgba(),
        "original color restored"
    );
}

/// WX4 AC3: an FxBlast message spawns one expanding ring that despawns
/// within ~0.5s; zero-radius blasts (per-victim feedback) spawn nothing.
#[test]
fn blast_rings_spawn_fade_and_skip_zero_radius() {
    let mut app = test_app();
    app.update();
    let rings = |app: &mut App| -> usize {
        let mut q = app.world_mut().query_filtered::<&BlastRing, ()>();
        q.iter(app.world()).count()
    };
    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<FxBlast>>()
        .write(FxBlast { position: Vec3::ZERO, radius: 0.0 });
    app.update();
    assert_eq!(rings(&mut app), 0, "zero-radius blast spawns no ring");

    app.world_mut()
        .resource_mut::<bevy::ecs::message::Messages<FxBlast>>()
        .write(FxBlast { position: Vec3::ZERO, radius: 3.0 });
    app.update();
    app.update(); // spawn lands via commands
    assert_eq!(rings(&mut app), 1, "blast spawns one ring");
    for _ in 0..35 {
        app.update(); // ~0.58s > 0.5s lifetime
    }
    assert_eq!(rings(&mut app), 0, "the ring fades within ~0.5s");
}

/// WX4 AC4: a death shell dissolves away within ~0.4s.
#[test]
fn death_shell_dissolves() {
    let mut app = test_app();
    app.update();
    {
        let mut meshes = std::mem::take(&mut *app.world_mut().resource_mut::<Assets<Mesh>>());
        let mut materials =
            std::mem::take(&mut *app.world_mut().resource_mut::<Assets<StandardMaterial>>());
        let mut queue = bevy::ecs::world::CommandQueue::default();
        {
            let mut commands = Commands::new(&mut queue, app.world());
            spawn_death_shell(
                &mut commands,
                &mut meshes,
                &mut materials,
                &Transform::from_xyz(1.0, 0.4, 1.0),
                Color::srgb(0.8, 0.2, 0.2),
            );
        }
        queue.apply(app.world_mut());
        *app.world_mut().resource_mut::<Assets<Mesh>>() = meshes;
        *app.world_mut().resource_mut::<Assets<StandardMaterial>>() = materials;
    }
    let shells = |app: &mut App| -> usize {
        let mut q = app.world_mut().query_filtered::<&DeathShell, ()>();
        q.iter(app.world()).count()
    };
    assert_eq!(shells(&mut app), 1, "shell spawned");
    for _ in 0..30 {
        app.update(); // ~0.5s > 0.4s lifetime
    }
    assert_eq!(shells(&mut app), 0, "the shell dissolves within ~0.4s");
}
