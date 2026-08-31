//! TuningPlugin (card 11 EguiTunePanel): F1 toggles an egui panel that hot-tunes
//! the `Balance` resource and the player's speed while the game runs. Mounted
//! only on the real app (`build_app`) — headless tests carry no egui dependency;
//! they exercise `Balance` directly (acceptance 2). API verified against local
//! bevy_egui-0.42.0 sources: `EguiContexts::ctx_mut()` + `egui::Window`.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::components::Player;
use crate::resources::Balance;

pub struct TuningPlugin;

impl Plugin for TuningPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TunePanelOpen>()
            // bevy_egui 0.42: UI systems MUST run inside the egui pass schedule
            // (plain Update races the internal context loop and panics with
            // "Dropped TexturesDelta" — found on first real-machine run).
            .add_systems(EguiPrimaryContextPass, tuning_panel);
    }
}

/// Whether the F1 panel is currently shown (starts closed).
#[derive(Resource, Default)]
struct TunePanelOpen {
    open: bool,
}

fn tuning_panel(
    keys: Res<ButtonInput<KeyCode>>,
    mut ctxs: EguiContexts,
    mut panel: ResMut<TunePanelOpen>,
    mut balance: ResMut<Balance>,
    mut players: Query<&mut Player>,
) {
    if keys.just_pressed(KeyCode::F1) {
        panel.open = !panel.open;
    }
    if !panel.open {
        return;
    }
    let Ok(ctx) = ctxs.ctx_mut() else {
        return; // no window/primary context yet (e.g. headless)
    };

    egui::Window::new("⚖ Balance").show(ctx, |ui| {
        ui.label("Phase-2 hot tuning (1.0 = weapon-table value, card 29)");
        ui.separator();
        ui.add(egui::Slider::new(&mut balance.slash_damage_scale, 0.2..=3.0).text("slash damage ×"));
        ui.add(
            egui::Slider::new(&mut balance.slash_cooldown_scale, 0.2..=3.0)
                .text("slash cooldown ×"),
        );
        ui.separator();
        ui.add(egui::Slider::new(&mut balance.nova_radius, 0.5..=4.0).text("nova radius"));
        ui.add(egui::Slider::new(&mut balance.nova_damage, 10.0..=200.0).text("nova damage"));
        ui.add(egui::Slider::new(&mut balance.nova_cooldown, 0.5..=15.0).text("nova cooldown s"));
        ui.separator();
        ui.add(egui::Slider::new(&mut balance.contact_damage, 1.0..=60.0).text("contact damage"));
        // Component-carried value: edit Player.speed directly (move_player untouched).
        for mut player in &mut players {
            ui.add(egui::Slider::new(&mut player.speed, 1.0..=12.0).text("player speed"));
        }
    });
}
