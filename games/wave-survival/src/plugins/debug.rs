//! DebugPlugin: observation channel — see what the game is doing (works headless).
//!   - Log dashboard every 2s: fps / state / entity count (RUST_LOG=info)
//!   - F12 screenshot to ./screenshot.png (acceptance evidence)

use bevy::{
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
};

use crate::states::GameState;

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
    state: Res<State<GameState>>,
    entities: Query<Entity>,
    mut log: ResMut<FpsLog>,
) {
    log.acc += time.delta_secs();
    if log.acc < 2.0 {
        return;
    }
    log.acc = 0.0;
    info!(
        "[dash] fps≈{:.0} state={:?} entities={}",
        1.0 / time.delta_secs().max(1e-6),
        state.get(),
        entities.iter().count()
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
