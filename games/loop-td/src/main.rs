//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).
//!
//! RPL2 flags:
//! - `--record`                 record the next match -> runs/<ts>.jsonl
//! - `--replay <file>`          headless replay verification (exit 0/1)
//! No flags: the normal game path, unchanged.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --replay <file>: verification entry — never opens a window. The map is
    // resolved like the game does (--map / LOOP_TD_MAP / maps/loop.ron) and
    // checked against the log's fingerprint.
    if let Some(file) = arg_value(&args, "--replay") {
        let map = loop_td::systems::gameplay::default_map_path();
        match loop_td::replay_io::replay_file(std::path::Path::new(&file), &map) {
            Ok(chain) => {
                let last = chain
                    .last()
                    .map(|(f, h)| format!(" (last checkpoint frame {f}, hash {h:#x})"))
                    .unwrap_or_default();
                println!("[loop-td] replay OK: {} hash checkpoints reconcile{last}", chain.len());
            }
            Err(e) => {
                eprintln!("[loop-td] replay FAILED: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let mut app = loop_td::build_app();
    if args.iter().any(|a| a == "--record") {
        app.add_plugins(loop_td::replay_io::ReplayIoPlugin { map_path: None });
        println!("[loop-td] recording on: the match will be written to runs/<timestamp>.jsonl when it ends");
    }
    app.run();
}

/// Value of `--flag <value>` from the arg list, if present.
fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}
