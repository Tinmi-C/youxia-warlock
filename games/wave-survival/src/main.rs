//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).

fn main() {
    wave_survival::build_app().run();
}
