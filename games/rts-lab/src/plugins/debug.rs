//! DebugPlugin: observation channel — works headless.
//!   - Log dashboard every 2s: fps / entity count (RUST_LOG=info)
//!   - F12 screenshot to ./screenshot.png (acceptance evidence)

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};
use warbase::prelude::{AgentOrderState, Carrying, MoveAgent, Order, Population, Resources};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FpsLog>()
            .add_systems(Update, (log_dashboard, screenshot_on_f12));
    }
}

#[derive(Resource, Default)]
struct FpsLog {
    acc: f32,
}

fn log_dashboard(
    time: Res<Time>,
    entities: Query<Entity>,
    res: Option<Res<Resources>>,
    pop: Option<Res<Population>>,
    workers: Query<(
        &Transform,
        &MoveAgent,
        &AgentOrderState,
        Option<&Carrying>,
    )>,
    mut log: ResMut<FpsLog>,
) {
    log.acc += time.delta_secs();
    if log.acc < 2.0 {
        return;
    }
    log.acc = 0.0;
    let (gold, used, cap) = match (res, pop) {
        (Some(res), Some(pop)) => (res.gold, pop.used, pop.cap),
        _ => (0, 0, 0), // before the map assembles
    };
    // First-worker probe: where is it, what is it doing (harvest observability;
    // the base Carrying component persists at 0 between trips — show units).
    let probe = workers.iter().next().map(|(tf, agent, state, carrying)| {
        format!(
            " worker@({:.1},{:.1}) order={:?} state={:?} wp={} to={:?} carry={}",
            tf.translation.x,
            tf.translation.z,
            agent.order.as_ref().map(|o| match o {
                Order::Move { .. } => "Move",
                Order::Stop => "Stop",
                Order::Harvest { .. } => "Harvest",
                Order::ReturnCargo { .. } => "ReturnCargo",
                _ => "other",
            }),
            state,
            agent.waypoints.len(),
            agent.waypoints.first().copied(),
            carrying.map(|c| c.0).unwrap_or(0),
        )
    });
    info!(
        "[dash] fps≈{:.0} entities={} gold={} pop={}/{}{}",
        1.0 / time.delta_secs().max(1e-6),
        entities.iter().count(),
        gold,
        used,
        cap,
        probe.unwrap_or_default()
    );
}

fn screenshot_on_f12(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::F12) {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk("screenshot.png"));
        info!("[dash] screenshot saved to screenshot.png");
    }
}
