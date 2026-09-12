//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).

fn main() {
    let mut app = warden::build_app();
    // Debug builds: apply --scenario NAME --seed N before run (DB2). The seed
    // must land before Startup so the opening hand replays; the scenario is
    // queued and loads on the first Update frame. Release builds have no CLI.
    #[cfg(debug_assertions)]
    warden::systems::debug::apply_cli_overrides(&mut app);
    app.run();
}
