//! Entry point — only assembles and runs the app.
//! Business logic lives in plugins/systems (see `lib.rs::build_app`).
//!
//! RPL2 flags:
//! - `--record`                 record the next match -> runs/<ts>.jsonl
//! - `--replay <file>`          headless replay verification (exit 0/1)
//! - `--spectate <file> [--speed N]`  real-render replay viewer (SEAM3c)
//! UI-DSGN flags:
//! - `--ui-mock <A|B|C>`        UI proposal mock scene -> ui-proposals/ PNGs
//! - `--ui-mock v2`             LT2-UI acceptance shots -> ui-proposals/lt2-final/
//! UITEX flags:
//! - `--art [--art-manifest <ron>]`  textured HUD (pilot of the SEAM4b
//!   seam). Default manifest `assets/ui/uitextures.ron`; any miss (no
//!   manifest / broken / missing file) warns and stays greybox.
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
    // --spectate <file> [--speed N]: the real-render replay viewer (SEAM3c).
    if let Some(file) = arg_value(&args, "--spectate") {
        let speed: f32 = arg_value(&args, "--speed")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        let mut app = loop_td::build_app();
        app.add_plugins(loop_td::plugins::spectate::SpectateIoPlugin {
            log_path: std::path::PathBuf::from(file),
            speed,
        });
        println!("[loop-td] spectate: re-simulating the recording at x{speed} (keys 1/2/3 change speed)");
        app.run();
        return;
    }
    let mut app = loop_td::build_app();
    if args.iter().any(|a| a == "--record") {
        app.add_plugins(loop_td::replay_io::ReplayIoPlugin { map_path: None });
        println!("[loop-td] recording on: the match will be written to runs/<timestamp>.jsonl when it ends");
    }
    // --art (UITEX pilot): load the UI texture manifest and flip ArtMode —
    // but only when EVERY declared texture actually loaded; any miss keeps
    // the HUD greybox (a warn per failure, never a panic / missing-texture
    // checkerboard).
    if args.iter().any(|a| a == "--art") {
        let manifest = arg_value(&args, "--art-manifest")
            .unwrap_or_else(|| format!("{}/assets/ui/uitextures.ron", env!("CARGO_MANIFEST_DIR")));
        let report =
            warbase::ui::load_ui_textures(app.world_mut(), std::path::Path::new(&manifest));
        if report.manifest_ok && report.failed.is_empty() && !report.loaded.is_empty() {
            app.insert_resource(warbase::ui::ArtMode(true));
            println!("[loop-td] art mode on: {} texture(s) from {manifest}", report.loaded.len());
        } else {
            println!(
                "[loop-td] art mode fallback to greybox (manifest_ok={}, loaded={:?}, failed={:?})",
                report.manifest_ok, report.loaded, report.failed
            );
        }
    }
    // --ui-mock <A|B|C|v2>: proposal shots or the LT2-UI acceptance suite
    // (frame-scripted, self-exits).
    if let Some(plan) = arg_value(&args, "--ui-mock") {
        let plan = plan.chars().next().unwrap_or('A').to_ascii_uppercase();
        if plan == '2' || plan.eq_ignore_ascii_case(&'v') {
            println!("[loop-td] ui-mock: v2 LT2-UI acceptance -> ui-proposals/lt2-final/");
            app.add_plugins(loop_td::plugins::ui_mock_v2::UiMockV2Plugin);
        } else {
            println!("[loop-td] ui-mock: plan {plan} -> ui-proposals/plan-{plan}/");
            app.add_plugins(loop_td::plugins::ui_mock::UiMockPlugin { plan });
        }
    }
    app.run();
}

/// Value of `--flag <value>` from the arg list, if present.
fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}
