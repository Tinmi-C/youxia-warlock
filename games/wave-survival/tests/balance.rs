//! Headless balance readout tests (docs/balance-anchors.md §4).
//!
//! Not a regression for a gameplay card — this is the observation channel that
//! turns the definition tables + wave formulas into objective per-wave metrics
//! so a human or AI can tune toward an anchor instead of guessing. The tests
//! verify the readout is *faithful* to the same data the game spawns with
//! (source-independent invariants), and print the full readout for inspection.

use wave_survival::components::MonsterKind;
use wave_survival::resources::Balance;
use wave_survival::systems::balance_audit::{
    check_anchors, natural_survival_ceiling, player_dps, report_all_waves, wave_readout,
    wave_readout_with, AnchorStatus, Overrides, MULTI_HIT_ESTIMATE, PLAYER_HP,
};
use wave_survival::systems::wave::{kinds_for_wave, wave_hp, wave_speed, SPAWN_RADIUS};
use wave_survival::systems::contact; // CONTACT_DAMAGE / INVULN_TIME

/// The readout must re-derive the same numbers the game itself uses. These are
/// source-consistency invariants (hold for any tuning), not pinned design
/// values — so retuning a wave formula or a multiplier never turns this red,
/// only a genuine misread of the source would.
#[test]
fn readout_is_faithful_to_game_sources() {
    let balance = Balance::default();
    let dps = player_dps(&balance);

    // dps = 34 / 0.45 (IronSword, default scale 1.0).
    assert!((dps - 34.0 / 0.45).abs() < 1e-3, "dps {dps}");

    for n in 1..=15u32 {
        let r = wave_readout(n, &balance);
        let kinds = kinds_for_wave(n);

        // Composition: the readout's count matches the spawned kind list.
        assert_eq!(r.count as usize, kinds.len(), "n={n}: count mismatch");

        // Total HP = Σ(wave_hp × kind.hp_mul) over the spawned kinds.
        let base_hp = wave_hp(n);
        let expected_hp: f32 = kinds.iter().map(|k| base_hp * k.hp_mul()).sum();
        assert!(
            (r.total_hp - expected_hp).abs() < 1e-2,
            "n={n}: total_hp {} vs {expected_hp}",
            r.total_hp
        );

        // Per-kind: ttk = hp/dps, approach = SPAWN_RADIUS/speed.
        for k in &r.kinds {
            assert!((k.ttk - k.hp / dps).abs() < 1e-3, "n={n}: ttk");
            let speed = base_speed_for(n, k.kind);
            assert!(
                (k.approach - SPAWN_RADIUS / speed).abs() < 1e-3,
                "n={n}: approach"
            );
            assert!(
                (k.approach_over_ttk - k.approach / k.ttk).abs() < 1e-3,
                "n={n}: ratio"
            );
        }

        // Clear times and survival follow from the above, not a second model.
        assert!((r.clear_single - r.total_hp / dps).abs() < 1e-2, "n={n}: clear_single");
        assert!(
            (r.clear_multi - r.total_hp / (dps * MULTI_HIT_ESTIMATE)).abs() < 1e-2,
            "n={n}: clear_multi"
        );
        assert!(
            (r.survival_bites - PLAYER_HP / contact::CONTACT_DAMAGE).abs() < 1e-3,
            "n={n}: bites"
        );
        assert!(
            (r.survival_seconds - r.survival_bites * contact::INVULN_TIME).abs() < 1e-3,
            "n={n}: seconds"
        );
    }
}

/// The anchors distinguish the playable window from the difficulty ramp:
/// within the window (w≤5) no anchor may be Fail, and the tank — which is the
/// only kind the N4 band colors before the window ends — reaches its (recalibrated
/// ≥1.0) band at w5 after the speed_mul 0.6→0.5 tweak. Beyond the window
/// (w6+), every anchor is Tbd, not Fail, so the readout doesn't cry
/// "unbalanced" at waves where the player is meant to lose.
#[test]
fn anchor_respects_playable_window() {
    let balance = Balance::default();

    // Within the window: nothing fails.
    for n in 1..=5u32 {
        for c in check_anchors(&wave_readout(n, &balance)) {
            assert_ne!(c.status, AnchorStatus::Fail, "n={n} {} must not fail in the window", c.name);
        }
    }

    // The tank reaches its band at w5 (the only tank wave inside the window):
    // slower tank (0.5) + recalibrated band ≥1.0 → pass.
    let w5 = check_anchors(&wave_readout(5, &balance));
    let w5_tank = w5
        .iter()
        .find(|c| c.name == "N4 tank_ratio")
        .unwrap();
    assert_eq!(w5_tank.status, AnchorStatus::Pass, "N4 should hold at w5 after the fix");
    assert!(w5_tank.value >= 1.0, "tank ratio {} should be ≥1.0", w5_tank.value);

    // Beyond the window: the ramp — every anchor is Tbd, not a false Fail.
    for n in [6u32, 7, 8, 10, 12] {
        for c in check_anchors(&wave_readout(n, &balance)) {
            assert_eq!(c.status, AnchorStatus::Tbd, "n={n} {} should be Tbd (ramp)", c.name);
        }
    }
}

/// The what-if overrides still turn a reading into a verdict within the window:
/// doubling the grunt HP pushes N2 (normal-grunt TTK) from Pass to Fail, so an
/// AI can probe a candidate knob without touching game code.
#[test]
fn what_if_override_flips_grunt_anchor() {
    let balance = Balance::default();

    let base = check_anchors(&wave_readout(5, &balance));
    let base_n2 = base.iter().find(|c| c.name == "N2 grunt_ttk").unwrap();
    assert_eq!(base_n2.status, AnchorStatus::Pass, "precondition: N2 passes at w5");

    let o = Overrides {
        grunt_hp_mul: Some(2.0),
        ..Default::default()
    };
    let tuned = check_anchors(&wave_readout_with(5, &balance, &o));
    let tuned_n2 = tuned.iter().find(|c| c.name == "N2 grunt_ttk").unwrap();
    assert_eq!(
        tuned_n2.status,
        AnchorStatus::Fail,
        "N2 should fail after doubling grunt hp, got value {}",
        tuned_n2.value
    );
    assert!(tuned_n2.value > 1.3, "grunt ttk {} should exceed 1.3", tuned_n2.value);
}

/// The difficulty gradient is inferred, not assumed: the single-target floor
/// and ×MULTI_HIT_ESTIMATE ceiling bracket where the wildcard "stands and
/// fights and dies". Verified against the readout it derives from.
#[test]
fn survival_ceiling_is_inferred_from_readout() {
    let balance = Balance::default();
    let c = natural_survival_ceiling(15, &balance);
    assert_eq!(c.single, Some(5), "clear_single first >6s at wave 5 (9.53s)");
    assert_eq!(c.multi, Some(6), "clear_multi first >6s at wave 6 (6.75s)");
}

fn base_speed_for(n: u32, kind: MonsterKind) -> f32 {
    wave_speed(n) * kind.speed_mul()
}

/// Print the readout for human/AI inspection: `cargo test -- --nocapture`.
#[test]
fn print_balance_readout() {
    let balance = Balance::default();
    print!("{}", report_all_waves(10, &balance));
}
