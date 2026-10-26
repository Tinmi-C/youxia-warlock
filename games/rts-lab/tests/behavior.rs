//! rts-lab behavior acceptance (RL series, ADR-0016 §11): drive the thin
//! game layer over warbase mechanisms headlessly. RL0 ships the smoke
//! skeleton; RL1 lands the first acceptance battery (map assembly / train /
//! harvest loop / selection / last-team-standing).

use warbase::prelude::*;
use warbase::testing::test_app;

/// RL0 smoke: warbase::testing::test_app() already wires WarbasePlugin with
/// a fixed timestep; the base resources exist and it updates headless.
#[test]
fn warbase_wires_headless() {
    let mut app = test_app();
    for _ in 0..5 {
        app.update();
    }
    assert!(app.world().contains_resource::<Resources>());
    assert!(app.world().contains_resource::<SelectionState>());
}
