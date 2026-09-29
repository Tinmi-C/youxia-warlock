pub mod acquisition;
pub mod camera;
pub mod command;
// Unified damage pipeline (SK7): DamageRequest in, single Enemy.hp writer,
// DamageDealt out.
pub mod damage;
// Debug cheat core (DB1/DB2): the whole module is debug-only — release
// builds have zero debug surface (panel, cheats, scenarios).
#[cfg(debug_assertions)]
pub mod debug;
// Debug time control + info overlays (DB3): same debug-only gate.
#[cfg(debug_assertions)]
pub mod debug_time;
pub mod enemy;
pub mod fx;
pub mod hud;
pub mod input;
pub mod l10n;
pub mod map;
pub mod meta;
pub mod pointer;
pub mod projectiles;
// Showroom single-exhibit browser (SM1): same debug-only gate as DB1-3.
#[cfg(debug_assertions)]
pub mod showroom;
// Shooting range (SM3R): same debug-only gate.
#[cfg(debug_assertions)]
pub mod range;
pub mod skills;
// Archer tower skill kit (SK9) — first tower-owned consumer of the framework.
pub mod archer_skills;
// Stat modification pipeline (SK6): StatMods -> EffectiveStats aggregation.
pub mod stats;
pub mod tower;
pub mod wave;
