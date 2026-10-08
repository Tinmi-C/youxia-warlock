//! BX0 acceptance: the plugin assembles on a minimal headless app and the
//! WarbaseSet chain runs in contract order on every frame.

use bevy::{prelude::*, MinimalPlugins};
use warbase::prelude::*;

#[derive(Resource, Default)]
struct Probe(Vec<&'static str>);

fn probe(tag: &'static str) -> impl FnMut(ResMut<Probe>) {
    move |mut p: ResMut<Probe>| p.0.push(tag)
}

#[test]
fn warbase_sets_run_in_contract_order_every_frame() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins(WarbasePlugin);
    app.init_resource::<Probe>();
    app.add_systems(
        Update,
        (
            probe("tick").in_set(WarbaseSet::Tick),
            probe("director").in_set(WarbaseSet::Director),
            probe("intent").in_set(WarbaseSet::Intent),
            probe("act").in_set(WarbaseSet::Act),
            probe("resolve").in_set(WarbaseSet::Resolve),
            probe("cleanup").in_set(WarbaseSet::Cleanup),
        ),
    );

    for _ in 0..3 {
        app.update();
    }

    let frame = ["tick", "director", "intent", "act", "resolve", "cleanup"];
    let expected: Vec<&str> = frame.into_iter().cycle().take(18).collect();
    assert_eq!(app.world().resource::<Probe>().0, expected);
}
