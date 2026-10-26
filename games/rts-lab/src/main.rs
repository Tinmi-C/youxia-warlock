//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).

fn main() {
    let mut app = rts_lab::build_app();
    app.run();
}
