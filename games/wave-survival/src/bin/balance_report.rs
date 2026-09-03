//! Command-line balance readout: `cargo run --bin balance_report [max_wave] [overrides]`.
//!
//! Prints the per-wave difficulty metrics + anchor verdicts
//! (docs/balance-anchors.md §4). It reuses the SAME sources the game uses
//! (wave formulas, MonsterKind/WeaponKind tables, Balance, contact constants),
//! so it is an observation channel, not a separate balance model.
//!
//! What-if overrides (forecast a tuning change WITHOUT editing game code):
//!   --grunt-hp <mul>  --runner-speed <mul>  --tank-speed <mul>  --tank-hp <mul>
//! e.g. `cargo run --bin balance_report 10 --tank-speed 0.45`
//!
//! No Bevy window / renderer: it only reads the lib's pure analysis functions.

use std::env;

use wave_survival::systems::balance_audit::Overrides;

fn main() {
    let mut max_wave: u32 = 10;
    let mut o = Overrides::default();
    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        // Only treat the next argument as a value when `a` is a known override
        // key — otherwise an override flag after a positional wave count would
        // be swallowed (parse bug caught in review).
        match a.as_str() {
            "--grunt-hp" | "--runner-speed" | "--tank-speed" | "--tank-hp" => {
                if let Some(v) = args.next() {
                    if let Ok(x) = v.parse::<f32>() {
                        match a.as_str() {
                            "--grunt-hp" => o.grunt_hp_mul = Some(x),
                            "--runner-speed" => o.runner_speed_mul = Some(x),
                            "--tank-speed" => o.tank_speed_mul = Some(x),
                            "--tank-hp" => o.tank_hp_mul = Some(x),
                            _ => unreachable!(),
                        }
                    }
                }
            }
            _ => {
                if let Ok(n) = a.parse::<u32>() {
                    max_wave = n;
                }
            }
        }
    }

    let balance = wave_survival::resources::Balance::default();
    print!(
        "{}",
        wave_survival::systems::balance_audit::report_all_waves_with(max_wave, &balance, &o)
    );
}
