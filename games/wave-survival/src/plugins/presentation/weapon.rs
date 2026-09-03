//! Presentation domain — carried-weapon attachment (card 30).

use bevy::prelude::*;

use crate::components::{EquippedWeapon, Player, WeaponKind, WeaponVisual};

use super::anim_runtime::AnimLink;
use super::PresentationSet;

// --- constants -------------------------------------------------------------

const IRON_SWORD_GLB: &str = "models/iron_sword.glb";
const GLAIVE_GLB: &str = "models/glaive.glb";
/// The hand bone node the weapon rides (verified offline in Blender).
const HAND_BONE: &str = "mixamorig:RightHand";

/// Which glTF a weapon row maps to (card 30).
fn weapon_glb(kind: WeaponKind) -> &'static str {
    match kind {
        WeaponKind::IronSword => IRON_SWORD_GLB,
        WeaponKind::Glaive => GLAIVE_GLB,
    }
}

// --- plugin ----------------------------------------------------------------

/// Weapon domain plugin.
pub struct WeaponPlugin;

impl Plugin for WeaponPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, attach_weapon.in_set(PresentationSet::Weapon))
            .add_systems(Update, weapon_scale_fixup.in_set(PresentationSet::WeaponFix));
    }
}

// --- helpers / systems -----------------------------------------------------

/// Card 30: find a named bone node inside a model subtree (helper kept `pub`
/// so headless tests can pin the lookup against a fake hierarchy).
pub fn find_bone_in_subtree(world: &World, wrapper: Entity, bone: &str) -> Option<Entity> {
    let mut stack: Vec<Entity> = world.get::<Children>(wrapper)?.iter().collect();
    while let Some(entity) = stack.pop() {
        if world
            .get::<Name>(entity)
            .map(|name| name.as_str() == bone)
            .unwrap_or(false)
        {
            return Some(entity);
        }
        if let Some(kids) = world.get::<Children>(entity) {
            stack.extend(kids.iter());
        }
    }
    None
}

/// Card 30: spawn the carried weapon's model as a child of the hero's right
/// hand BONE (mixamorig:RightHand) — the attack clip animation then carries the
/// sword through every swing for free. Swaps the mesh when the logical row
/// changes.
fn attach_weapon(
    mut commands: Commands,
    players: Query<Entity, (With<Player>, With<Children>)>,
    links: Query<(), With<AnimLink>>,
    children: Query<&Children>,
    names: Query<&Name>,
    equipped: Query<&EquippedWeapon>,
    mut visuals: Query<&mut WeaponVisual>,
    mut worlds: Query<&mut WorldAssetRoot>,
    assets: Res<AssetServer>,
) {
    let Ok(root) = players.single() else {
        return;
    };
    let Ok(kids) = children.get(root) else {
        return;
    };
    let wrapper = match kids.iter().find(|k| links.contains(*k)) {
        Some(e) => e,
        None => return, // model wrapper not spawned yet
    };
    let Ok(eq) = equipped.get(root) else {
        return;
    };
    // query-based bone search (a `&World` param would conflict with the two
    // &mut queries above — B0001); the World-based helper twin lives for tests
    let Some(hand) = children
        .iter_descendants(wrapper)
        .find(|n| names.get(*n).map(|name| name.as_str() == HAND_BONE).unwrap_or(false))
    else {
        return; // model subtree not bound yet
    };
    // one query for both detect and mutate (B0001: two WeaponVisual params
    // — read + write — would conflict)
    let existing = children
        .get(hand)
        .ok()
        .and_then(|wk| wk.iter().find(|k| visuals.contains(*k)));
    match existing {
        Some(kid) => {
            // same slot: swap the mesh only when the logical row changed
            if let Ok(mut vis) = visuals.get_mut(kid) {
                if vis.kind != eq.0 {
                    if let Ok(mut world) = worlds.get_mut(kid) {
                        world.0 =
                            assets.load(GltfAssetLabel::Scene(0).from_asset(weapon_glb(eq.0)));
                        vis.kind = eq.0;
                        info!("[presentation] weapon swapped to {:?}", eq.0);
                    }
                }
            }
        }
        None => {
            // grip sits at the bone origin; the fixup pass cancels the
            // accumulated bone scale so the weapon keeps its authored meters
            commands.spawn((
                ChildOf(hand),
                WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(weapon_glb(eq.0)))),
                WeaponVisual { kind: eq.0 },
                Transform::IDENTITY,
            ));
            info!("[presentation] weapon attached to {HAND_BONE}: {:?}", eq.0);
        }
    }
}

/// Card 30: the hand bone inherits the hero wrapper scale (1.353); cancel it
/// once so the weapon keeps its authored meters (sword 0.9 m in the world).
#[derive(Component)]
pub struct WeaponScaleFixed;

pub fn weapon_scale_fixup(
    mut commands: Commands,
    globals: Query<&GlobalTransform>,
    weapons: Query<(Entity, &ChildOf), (With<WeaponVisual>, Without<WeaponScaleFixed>)>,
    mut transforms: Query<&mut Transform>,
) {
    for (entity, child_of) in &weapons {
        let Ok(parent_gt) = globals.get(child_of.parent()) else {
            continue;
        };
        let parent_scale = parent_gt.scale();
        if parent_scale.x.abs() < 1e-6 {
            continue;
        }
        if let Ok(mut tf) = transforms.get_mut(entity) {
            tf.scale = Vec3::ONE / parent_scale;
            commands.entity(entity).insert(WeaponScaleFixed);
            info!("[presentation] weapon scale compensated (bone scale {parent_scale:?})");
        }
    }
}
