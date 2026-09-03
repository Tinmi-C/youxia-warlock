//! Card 33 verification: prove the animation playback mechanism actually drives
//! entities. The 59-regression suite never loads glTF and never exercises the
//! animation systems, so "does the clip animate" was never checked headlessly.
//! This test builds a real minimal animation (a translation wobble on one
//! animated target), runs Bevy's animation systems over a few frames, and
//! asserts the target's Transform actually moved — proof the clip is driving
//! the skeleton rather than idling.
//!
//! Run: `cargo test --test anim_playback`

use std::time::Duration;

use bevy::{
    animation::{
        animated_field, animation_curves::*, prelude::*,
        AnimationTargetId, AnimatedBy, RepeatAnimation,
    },
    app::App,
    asset::{AssetPlugin, Assets},
    ecs::name::Name,
    math::vec3,
    prelude::*,
    time::TimeUpdateStrategy,
    transform::TransformPlugin,
};
use bevy::math::curve::{FunctionCurve, Interval};

#[test]
fn animation_player_drives_target_transform() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(), // provides AssetServer (AnimationPlugin.init_asset needs it)
        AnimationPlugin, // advance_animations + animate_targets (headless-safe)
    ))
    .init_resource::<Assets<AnimationGraph>>()
    .init_resource::<Assets<AnimationClip>>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 60.0,
    )));

    // A target that the animation should move (Hips bone, same scheme the glTF
    // loader uses to mark skinned bones).
    let target_id = AnimationTargetId::from(&Name::new("Hips"));
    let target = app
        .world_mut()
        .spawn((Transform::from_xyz(0.0, 0.0, 0.0), target_id))
        .id();

    // One clip: translate the target along X by t over [0,1) (a wobble).
    let wobble = FunctionCurve::new(Interval::UNIT, |t| vec3(t * 1.0, 0.0, 0.0));
    let anim_curve = AnimatableCurve::new(animated_field!(Transform::translation), wobble);
    let mut clip = AnimationClip::default();
    clip.set_duration(1.0);
    clip.add_curve_to_target(target_id, anim_curve);
    let clip_handle = app.world_mut().resource_mut::<Assets<AnimationClip>>().add(clip);

    // A one-clip animation graph.
    let mut graph = AnimationGraph::default();
    let node = graph.add_clip(clip_handle, 1.0, graph.root);
    let graph_handle = app.world_mut().resource_mut::<Assets<AnimationGraph>>().add(graph);

    // The player drives the target via AnimatedBy.
    let player = app
        .world_mut()
        .spawn((AnimationPlayer::default(), AnimationGraphHandle(graph_handle)))
        .id();
    app.world_mut().entity_mut(target).insert((AnimatedBy(player),));

    // Play the clip, run several frames.
    {
        let mut players = app.world_mut().query::<&mut AnimationPlayer>();
        let mut p = players.single_mut(app.world_mut()).expect("one player");
        p.play(node).set_repeat(RepeatAnimation::Forever);
    }

    let start = app.world().entity(target).get::<Transform>().unwrap().translation;
    for _ in 0..30 {
        app.update();
    }
    let end = app.world().entity(target).get::<Transform>().unwrap().translation;

    assert!(
        (end.x - start.x).abs() > 0.05,
        "animation should have moved the target along X: start {start:?} -> end {end:?}"
    );
}
