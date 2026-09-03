//! Presentation domain — hit-flash emissive (card 14).

use std::collections::HashMap;

use bevy::{math::VectorSpace, prelude::*};

use crate::components::Visual;

use super::PresentationSet;

/// Lazily-created private material clones per owner, so emissive flash edits
/// never bleed between entities that share a (per-kind) base material. Entries
/// live for the session; a despawned owner leaves only a few orphaned assets —
/// bounded by the count of entities that ever flashed this run.
#[derive(Resource, Default)]
struct FlashAssets {
    privates: HashMap<Entity, Vec<Handle<StandardMaterial>>>,
}

/// Card 14 flash curve: emissive rests at BLACK and whiteouts as `flash`
/// approaches 1. Single source of truth — the original formula lerped from
/// `base_color`, which left a permanent additive self-glow at flash=0 and
/// washed every model out to flat gray (art regression, fixed 2026-08-28).
fn flash_emissive(flash: f32) -> LinearRgba {
    LinearRgba::BLACK.lerp(LinearRgba::WHITE, flash)
}

/// Flash domain plugin.
pub struct FlashPlugin;

impl Plugin for FlashPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlashAssets>()
            .add_systems(Update, apply_flash_visuals.in_set(PresentationSet::Flash));
    }
}

/// Card 14 (presentation half): mirror each owner's Visual.flash onto its model
/// materials as an emissive whiteout. The logic side owns the number; here we
/// only paint it — headless worlds stay untouched by construction.
fn apply_flash_visuals(
    mut commands: Commands,
    holders: Query<(Entity, &Visual)>,
    children: Query<&Children>,
    slots: Query<&MeshMaterial3d<StandardMaterial>>,
    meshes: Query<(), With<Mesh3d>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flash: ResMut<FlashAssets>,
) {
    for (root, vis) in &holders {
        // Privatize on first sight: clone current mesh-material handles into
        // per-owner instances and swap them in. Re-runs harmlessly until at
        // least one slot exists (models load a few frames after spawn).
        let entry = flash.privates.entry(root).or_default();
        if entry.is_empty() {
            let mut collected: Vec<Handle<StandardMaterial>> = Vec::new();
            for node in children.iter_descendants(root) {
                if meshes.contains(node) {
                    if let Ok(slot) = slots.get(node) {
                        let mut variant = match materials.get(&slot.0) {
                            Some(material) => material.clone(),
                            None => StandardMaterial::default(),
                        };
                        // rest-at-black: see flash_emissive docs
                        variant.emissive = flash_emissive(vis.flash);
                        let handle = materials.add(variant);
                        commands.entity(node).insert(MeshMaterial3d(handle.clone()));
                        collected.push(handle);
                    }
                }
            }
            *entry = collected;
            continue; // first pass already paints via the clones' initial state
        }
        // steady state: repaint private instances in place — no allocations,
        // and other entities sharing the pre-clone lineage are never touched.
        for handle in entry {
            if let Some(mut material) = materials.get_mut(handle) {
                material.emissive = flash_emissive(vis.flash);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_emissive_rests_at_black() {
        // THE regression: the old formula rested at base_color, adding a
        // permanent self-glow that washed every character to flat gray.
        let rest = flash_emissive(0.0);
        assert!(rest.red < 1e-6 && rest.green < 1e-6 && rest.blue < 1e-6);
    }

    #[test]
    fn flash_emissive_whiteouts_at_one() {
        let full = flash_emissive(1.0);
        assert!((full.red - 1.0).abs() < 1e-6);
        assert!((full.green - 1.0).abs() < 1e-6);
        assert!((full.blue - 1.0).abs() < 1e-6);
    }

    #[test]
    fn flash_emissive_is_linear_in_flash() {
        let quarter = flash_emissive(0.25);
        assert!((quarter.red - 0.25).abs() < 1e-6);
        assert!((quarter.blue - 0.25).abs() < 1e-6);
    }
}
