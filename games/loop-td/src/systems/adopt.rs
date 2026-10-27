//! LT-ADOPT (round-2 plan §4 line 2): loop-td adopts the round-1 base seams
//! it had not consumed yet. Three thin wires, no pipeline code (thin-game
//! rule): the digit-row modal yield, the TD selection kit for the mouse
//! chain, and the hero-tower Inventory. The keyboard build cursor stays a
//! parallel input mode — `gameplay::build_input` is untouched.

use bevy::prelude::*;
use warbase::prelude::*;

/// `ModalKeyClaim` owner id while the build menu is the active mode.
pub const BUILD_MENU_CLAIM: &str = "loop-td-build-menu";

/// PreUpdate: derive the digit-row claim from the ui selection mode — open
/// build menu claims the digits for the command card (the base keyboard
/// bridge then skips its digit -> `GroupCommand` translation, hole-8),
/// closing the menu releases OUR claim only (nested modals from later
/// increments keep theirs). Stateless per-frame reconciliation: every mode
/// writer (the B toggle, restarts, mock scenes) is covered without hooks,
/// and PreUpdate guarantees the claim is settled before the base input
/// bridges run in Update.
pub fn sync_modal_claim(
    selection: Res<warbase::ui::Selection>,
    mut claim: ResMut<ModalKeyClaim>,
) {
    let open = matches!(selection.mode, warbase::ui::SelectionMode::Build);
    if open {
        if claim.0.is_none() {
            claim.0 = Some(BUILD_MENU_CLAIM);
        }
    } else if claim.0 == Some(BUILD_MENU_CLAIM) {
        claim.0 = None;
    }
}

/// Update: TD selection kit on fresh spawns (every spawn path funnels here —
/// placement, upgrades, hero grants, restarts):
/// - `Selectable` on Defender-side units (towers and hero towers; creeps are
///   Attacker side and stay out of the pick chain), joining the base mouse
///   chain: PickAttempt point select, Aabb box select, right-click smart
///   commands;
/// - the base 6-slot `Inventory` on hero defs (`hero: true`) — the LT2
///   equipment-on-towers runway (same Added<UnitInstance> pattern as
///   rts-lab's attach_rts_kit).
pub fn attach_td_kit(
    added: Query<(Entity, &UnitInstance, &Allegiance), Added<UnitInstance>>,
    registry: Res<UnitRegistry>,
    mut commands: Commands,
) {
    for (entity, instance, side) in &added {
        if *side != Allegiance::Defender {
            continue; // creeps never join the selection chain
        }
        let Some(def) = registry.get(&instance.def) else { continue };
        let mut ec = commands.entity(entity);
        ec.insert(Selectable);
        if def.hero {
            ec.insert(Inventory::default());
        }
    }
}
