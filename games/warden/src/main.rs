//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).

fn main() {
    warden::build_app().run();
}
