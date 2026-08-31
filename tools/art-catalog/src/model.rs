//! Shared data model for the catalog (card AC-1).

use crate::json::Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Library,
    Game,
}

impl Domain {
    pub fn as_str(self) -> &'static str {
        match self {
            Domain::Library => "library",
            Domain::Game => "game",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Model,
    Texture,
    Audio,
    Font,
    Ui,
    Other,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Model => "model",
            Kind::Texture => "texture",
            Kind::Audio => "audio",
            Kind::Font => "font",
            Kind::Ui => "ui",
            Kind::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnimClip {
    pub name: String,
    pub strip: String, // repo-relative sprite strip PNG
    pub frames: f64,
}

#[derive(Debug, Clone)]
pub struct MetaInfo {
    pub source_gltf: String,
    pub meta_path: String,
    pub height_m: Option<f64>,
    pub triangles: Option<f64>,
    pub materials: Vec<String>,
    pub clips: Vec<String>,
    pub has_armature: bool,
    pub renders: Vec<String>, // repo-relative image paths
    pub anim: Vec<AnimClip>,  // per-clip preview strips (card AC-2)
}

#[derive(Debug, Clone)]
pub struct RefHit {
    pub file: String, // repo-relative
    pub line: usize,
    pub snippet: String,
}

/// Ground-truth facts parsed from the GLB container's own JSON chunk
/// (what is actually inside the runtime file, not what meta.json claims).
#[derive(Debug, Clone)]
pub struct GlbInfo {
    pub animations: Vec<String>,   // may contain empty names
    pub durations: Vec<f64>,       // seconds, parallel to animations
    pub skins: f64,
    pub meshes: f64,
}

/// A code reference to one animation clip of a model, found by heuristics:
/// `HERO_CLIP_*`-style constants and `*_clip()` definition-table functions.
#[derive(Debug, Clone)]
pub struct AnimRef {
    pub clip_index: usize,
    pub symbol: String,
    pub file: String, // repo-relative
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: String, // file stem for models, file name otherwise
    pub kind: Kind,
    pub domain: Domain,
    pub path: String, // repo-relative
    pub size: u64,
    pub modified: u64, // unix seconds
    pub meta: Option<MetaInfo>,
    pub glb: Option<GlbInfo>, // game-domain .glb models only
    pub anim_refs: Vec<AnimRef>,
    pub pipeline_status: Option<String>, // intake ticket state (card AC-3)
    pub refs: Vec<RefHit>,
    pub stale_reasons: Vec<String>,
}

impl Asset {
    pub fn file_name(&self) -> String {
        std::path::Path::new(&self.path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    }
    pub fn is_referenced(&self) -> bool {
        !self.refs.is_empty()
    }
    pub fn to_json(&self) -> Json {
        let meta = match &self.meta {
            Some(m) => Json::obj(vec![
                ("source", Json::s(&m.source_gltf)),
                ("height_m", m.height_m.map(Json::Num).unwrap_or(Json::Null)),
                ("triangles", m.triangles.map(Json::Num).unwrap_or(Json::Null)),
                (
                    "materials",
                    Json::Arr(m.materials.iter().map(|s| Json::s(s.clone())).collect()),
                ),
                (
                    "clips",
                    Json::Arr(m.clips.iter().map(|s| Json::s(s.clone())).collect()),
                ),
                ("has_armature", Json::b(m.has_armature)),
                (
                    "renders",
                    Json::Arr(m.renders.iter().map(|s| Json::s(s.clone())).collect()),
                ),
                (
                    "anim",
                    Json::Arr(
                        m.anim
                            .iter()
                            .map(|c| {
                                Json::obj(vec![
                                    ("name", Json::s(&c.name)),
                                    ("strip", Json::s(&c.strip)),
                                    ("frames", Json::n(c.frames)),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("meta_path", Json::s(&m.meta_path)),
            ]),
            None => Json::Null,
        };
        Json::obj(vec![
            ("id", Json::s(&self.id)),
            ("kind", Json::s(self.kind.as_str())),
            ("domain", Json::s(self.domain.as_str())),
            ("path", Json::s(&self.path)),
            ("size", Json::n(self.size as f64)),
            ("modified", Json::n(self.modified as f64)),
            ("meta", meta),
            (
                "glb",
                match &self.glb {
                    Some(g) => Json::obj(vec![
                        (
                            "animations",
                            Json::Arr(g.animations.iter().map(|s| Json::s(s.clone())).collect()),
                        ),
                        ("durations", Json::Arr(g.durations.iter().map(|d| Json::n(*d)).collect())),
                        ("skins", Json::n(g.skins)),
                        ("meshes", Json::n(g.meshes)),
                    ]),
                    None => Json::Null,
                },
            ),
            (
                "anim_refs",
                Json::Arr(
                    self.anim_refs
                        .iter()
                        .map(|r| {
                            Json::obj(vec![
                                ("clip_index", Json::n(r.clip_index as f64)),
                                ("symbol", Json::s(&r.symbol)),
                                ("file", Json::s(&r.file)),
                                ("line", Json::n(r.line as f64)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "pipeline_status",
                match &self.pipeline_status {
                    Some(s) => Json::s(s.clone()),
                    None => Json::Null,
                },
            ),
            (
                "refs",
                Json::Arr(
                    self.refs
                        .iter()
                        .map(|r| {
                            Json::obj(vec![
                                ("file", Json::s(&r.file)),
                                ("line", Json::n(r.line as f64)),
                                ("snippet", Json::s(&r.snippet)),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "stale_reasons",
                Json::Arr(self.stale_reasons.iter().map(|s| Json::s(s.clone())).collect()),
            ),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: &'static str, // "error" | "warning" | "info"
    pub subject: String,
    pub evidence: String,
    pub fix_hint: String,
}

impl Finding {
    pub fn to_json(&self) -> Json {
        Json::obj(vec![
            ("rule_id", Json::s(self.rule)),
            ("severity", Json::s(self.severity)),
            ("subject", Json::s(&self.subject)),
            ("evidence", Json::s(&self.evidence)),
            ("fix_hint", Json::s(&self.fix_hint)),
        ])
    }
}

#[derive(Debug, Clone)]
pub struct IntakeRequest {
    pub file: String,
    pub raw: Json, // full parsed document, re-emitted verbatim
}

#[derive(Debug, Clone)]
pub struct ScenarioCard {
    pub file: String,
    pub id: String,
    pub name: String,
    pub trigger: String,
    pub steps: usize,
    pub human_steps: usize,
    pub raw: Json,
}
