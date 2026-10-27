//! LT2-2 the artifact (v2 proposal §2 #2): a task chain — clear
//! `ARTIFACT_WAVES_REQUIRED` waves and kill `ARTIFACT_BOSSES_REQUIRED`
//! bosses — unlocks a TEAM-UNIQUE claim. Once claimed, every tower on the
//! field carries a permanent aura `ModEntry` whose source is the artifact
//! marker entity (`ClaimedArtifact`), re-upserted every frame so towers
//! placed later join automatically and sold towers take nothing stale with
//! them (upsert is idempotent; entries are `from_aura: false` so the base
//! aura rebuild never strips them).
//!
//! The aura amount is a game-side starting value (the map bundle has no
//! artifact section; engine seam growth would move it into the RON).

use bevy::prelude::*;
use warbase::prelude::*;

/// Waves to clear before the artifact may be claimed.
pub const ARTIFACT_WAVES_REQUIRED: u32 = 10;
/// Bosses to kill before the artifact may be claimed.
pub const ARTIFACT_BOSSES_REQUIRED: u32 = 2;
/// The claimed artifact's team-wide aura (起点值, tunable).
pub const ARTIFACT_AURA_KIND: StatKind = StatKind::DamageMult;
pub const ARTIFACT_AURA_AMOUNT: f32 = 0.15;

#[derive(Resource, Debug, Default)]
pub struct ArtifactQuest {
    pub waves_cleared: u32,
    pub boss_kills: u32,
    pub claimed: bool,
}

/// The spawned artifact marker entity — the StatMods source of the aura.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ClaimedArtifact(pub Entity);

#[derive(Resource, Default)]
struct ClearCursor(bevy::ecs::message::MessageCursor<WaveCleared>);

#[derive(Resource, Default)]
struct BossDeathCursor(bevy::ecs::message::MessageCursor<UnitDied>);

/// Update: count cleared waves and boss kills into the quest.
pub fn track_artifact_progress(world: &mut World) {
    world.get_resource_or_insert_with(ArtifactQuest::default);
    world.get_resource_or_insert_with(ClearCursor::default);
    world.get_resource_or_insert_with(BossDeathCursor::default);
    let clears: usize = world.resource_scope(|w, mut c: Mut<ClearCursor>| {
        let msgs = w.resource::<bevy::ecs::message::Messages<WaveCleared>>();
        c.0.read(msgs).count()
    });
    let bosses: usize = world.resource_scope(|w, mut c: Mut<BossDeathCursor>| {
        let msgs = w.resource::<bevy::ecs::message::Messages<UnitDied>>();
        c.0.read(msgs).filter(|d| d.def == "boss").count()
    });
    if clears + bosses == 0 {
        return;
    }
    let mut quest = world.resource_mut::<ArtifactQuest>();
    quest.waves_cleared += clears as u32;
    quest.boss_kills += bosses as u32;
    info!(
        "[loop-td] artifact quest: {}/{} waves, {}/{} bosses",
        quest.waves_cleared, ARTIFACT_WAVES_REQUIRED, quest.boss_kills, ARTIFACT_BOSSES_REQUIRED
    );
}

/// Claim the artifact: refuses with a readable reason until the chain is
/// complete, then spawns the artifact marker and books the claim (unique).
pub fn claim_artifact(world: &mut World) -> Result<(), String> {
    world.get_resource_or_insert_with(ArtifactQuest::default);
    if world.get_resource::<ClaimedArtifact>().is_some() {
        return Err("artifact already claimed".into());
    }
    let (waves, bosses) = {
        let q = world.resource::<ArtifactQuest>();
        (q.waves_cleared, q.boss_kills)
    };
    if waves < ARTIFACT_WAVES_REQUIRED || bosses < ARTIFACT_BOSSES_REQUIRED {
        return Err(format!(
            "quest incomplete ({waves}/{} waves, {bosses}/{} bosses)",
            ARTIFACT_WAVES_REQUIRED, ARTIFACT_BOSSES_REQUIRED
        ));
    }
    let marker = world.spawn_empty().id();
    world.insert_resource(ClaimedArtifact(marker));
    world.resource_mut::<ArtifactQuest>().claimed = true;
    info!("[loop-td] artifact claimed (aura on every tower)");
    Ok(())
}

/// Update: keep the team-wide aura entry present on every Defender tower
/// (source = the artifact entity; idempotent upsert, later towers included).
pub fn apply_artifact_aura(world: &mut World) {
    let Some(marker) = world.get_resource::<ClaimedArtifact>().copied() else { return };
    let mut targets: Vec<Entity> = Vec::new();
    {
        let mut q =
            world.query_filtered::<(Entity, &Allegiance), With<UnitInstance>>();
        for (e, side) in q.iter(world) {
            if *side == Allegiance::Defender {
                targets.push(e);
            }
        }
    }
    for e in targets {
        let Ok(mut holder) = world.get_entity_mut(e) else { continue };
        let mut mods = holder.entry::<StatMods>().or_default();
        mods.into_mut().upsert(ModEntry {
            source: marker.0,
            kind: ARTIFACT_AURA_KIND,
            amount: ARTIFACT_AURA_AMOUNT,
            expires_at: None,
            from_aura: false,
        });
    }
}

/// Minimal input entry: Q claims once the chain is complete.
pub fn artifact_input(world: &mut World) {
    if !world.contains_resource::<ButtonInput<KeyCode>>() {
        return;
    }
    if *world.resource::<MatchFlow>() != MatchFlow::Running {
        return;
    }
    let keys = world.resource::<ButtonInput<KeyCode>>().clone();
    if !keys.just_pressed(KeyCode::KeyQ) {
        return;
    }
    if let Err(e) = claim_artifact(world) {
        info!("[loop-td] artifact: {e}");
    }
}

/// Run-restart hook: the quest is per-match (marker despawned, resources
/// dropped; tower StatMods die with the towers restart_run despawns).
pub fn reset(world: &mut World) {
    if let Some(claimed) = world.get_resource::<ClaimedArtifact>().copied() {
        let _ = world.get_entity_mut(claimed.0).map(|mut e| e.despawn());
    }
    world.remove_resource::<ClaimedArtifact>();
    world.remove_resource::<ArtifactQuest>();
    world.remove_resource::<ClearCursor>();
    world.remove_resource::<BossDeathCursor>();
}
