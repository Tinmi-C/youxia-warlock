pub mod acquisition;
pub mod camera;
pub mod command;
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
pub mod skills;
pub mod tower;
pub mod wave;
