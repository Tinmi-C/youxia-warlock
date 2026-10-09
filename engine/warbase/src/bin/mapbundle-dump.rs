//! mapbundle-dump: print a MapBundle's unit defs as JSON (ADR-0015).
//!
//! Normalized input for `tools/art-needs` (the demand pipeline). Loading goes
//! through the real serde loader + validator, so a malformed or
//! version-mismatched map fails loudly here — the projection never lies.
//!
//! Usage:
//!     cargo run -p warbase --bin mapbundle-dump -- games/loop-td/maps/loop.ron
//!     cargo run -p warbase --bin mapbundle-dump --default-features=false -- <map.ron>
use std::path::PathBuf;

fn main() {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = match (args.next(), args.next()) {
        (Some(p), None) => p.into(),
        _ => {
            eprintln!("usage: mapbundle-dump <map.ron>");
            std::process::exit(2);
        }
    };
    match warbase::bundle::load_bundle(&path) {
        Ok(bundle) => print!("{}", warbase::bundle::dump_units_json(&bundle)),
        Err(e) => {
            eprintln!("mapbundle-dump: {path:?}: {e}");
            std::process::exit(1);
        }
    }
}
