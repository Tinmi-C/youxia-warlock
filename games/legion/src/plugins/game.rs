//! GamePlugin: headless-safe core — the Plan-A skin (LG3) + per-player
//! wallets + the round layer (hire shop / send shop / leak rule + breach
//! bounty / round clock with the 20-round cap) + unit kit attachment + the
//! LG3 panel suite (HUD 资源条/命数板/商店卡/布阵高亮/发兵入口/回合横幅).
//! Rendering-only deps stay in VisualPlugin.

use bevy::prelude::*;
use warbase::prelude::cleanup_dead;
use warbase::WarbaseSet;

use crate::resources::{PendingSends, SendLedger};
use crate::systems::{gameplay, round};

/// Headless-safe core (input bridges no-op without ButtonInput).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        // LG3: the boarded Plan-A "Iron & Gold" skin (loop-td's RON, copied
        // verbatim) — every panel (these six + the warbase command surfaces)
        // reads tokens from it; swapping the resource/RON re-skins the whole
        // HUD with zero code change (asserted by tests/ui_panels.rs).
        app.insert_resource(crate::systems::theme::plan_a());
        // Game-side messages (base ones are registered by WarbasePlugin).
        app.add_message::<round::HireCommand>()
            .add_message::<round::HireRejected>()
            .add_message::<round::SendCommand>()
            .add_message::<round::SendRejected>()
            .add_message::<round::PhaseChanged>()
            .add_message::<round::RoundSettled>()
            .add_message::<round::LeakHappened>()
            .add_message::<round::PlayerEliminated>()
            .init_resource::<SendLedger>()
            .init_resource::<PendingSends>()
            // LG3: the six-panel Plan-A HUD (headless-safe; the legacy
            // single-line debug HUD is retired with it).
            .add_plugins(crate::systems::panels::PanelsPlugin);

        app.add_systems(
            Startup,
            // Skills-free map: assemble first (starting_forces spawn inside
            // assemble), then wallets, then the round state (lanes/slots/
            // lives/pop derive from the assembled world).
            (gameplay::load_map, gameplay::init_purses, round::init_round_state).chain(),
        )
        .add_systems(
            Update,
            (
                gameplay::attach_legion_kit,
                // The hire and send shops run ahead of the base tick (chained:
                // both hold ResMut<Purse>; order-inert for message reads,
                // keeps purchases early in the frame).
                (round::hire, round::send).chain(),
                // Input bridges (headless no-ops without ButtonInput).
                (round::hire_input, round::skip_build_input, round::send_input),
                // Bounty credit must run INSIDE Cleanup, before the base
                // cleanup_dead despawns the killer: UnitDied.killer is an
                // Entity reference and a mutual kill's killer is Dead-marked
                // the same frame (hole L6 — no documented seam for reading
                // killer attribution post-cleanup).
                round::bounty_credit
                    .in_set(WarbaseSet::Cleanup)
                    .before(cleanup_dead),
                // Round layer observes the settled frame: leaks first (they
                // can eliminate a player), then the pop recount, then the
                // round clock (battle orders / settlement).
                (
                    round::leak_tick,
                    round::recount_population,
                    round::round_tick,
                )
                    .chain()
                    .after(WarbaseSet::Cleanup),
            ),
        );
    }
}
