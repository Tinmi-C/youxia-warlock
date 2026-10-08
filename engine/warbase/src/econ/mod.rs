//! Economy (BX7): gold/wood/population through a single command stream —
//! same single-writer idea as the damage pipeline. Kill bounties flow in
//! here too (UnitDied -> Grant), so readings and cheat panels come free.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EconStart {
    pub gold: u32,
    pub wood: u32,
    pub pop_cap: u8,
}

#[derive(Debug, Clone, Resource, Serialize, Deserialize)]
pub struct EconRules {
    /// Gold per wood when converting.
    pub wood_to_gold: u32,
    /// Wood cost of the first population purchase; grows multiplicatively.
    pub pop_cost_base: u32,
    pub pop_cost_growth: f32,
    pub pop_gain: u8,
}

impl Default for EconRules {
    fn default() -> Self {
        Self { wood_to_gold: 25, pop_cost_base: 2, pop_cost_growth: 1.5, pop_gain: 2 }
    }
}

#[derive(Resource, Debug, Default)]
pub struct Resources {
    pub gold: u32,
    pub wood: u32,
}

#[derive(Resource, Debug, Default)]
pub struct Population {
    pub used: u8,
    pub cap: u8,
}

/// How many population purchases happened (drives the cost curve).
#[derive(Resource, Debug, Default)]
pub struct PopPurchases(pub u32);

#[derive(Message, Debug)]
pub enum EconCommand {
    Spend { gold: u32, wood: u32 },
    Grant { gold: u32, wood: u32 },
    ConvertWood { wood: u32 },
    BuyPopulation,
    /// Positive reserves, negative frees (build module pairs these with
    /// placement/selling).
    PopUse { delta: i8 },
}

#[derive(Message, Debug, Clone, Copy)]
pub struct EconChanged {
    pub gold: u32,
    pub wood: u32,
    pub pop_used: u8,
    pub pop_cap: u8,
}

#[derive(Message, Debug)]
pub struct EconRejected {
    pub reason: String,
}

fn snapshot(res: &Resources, pop: &Population) -> EconChanged {
    EconChanged { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap }
}

pub fn apply_econ(
    mut cmds: MessageReader<EconCommand>,
    mut deaths: MessageReader<crate::combat::UnitDied>,
    mut res: ResMut<Resources>,
    mut pop: ResMut<Population>,
    mut purchases: ResMut<PopPurchases>,
    rules: Res<EconRules>,
    mut changed: MessageWriter<EconChanged>,
    mut rejected: MessageWriter<EconRejected>,
) {
    // Kill bounties first so a same-frame Spend can rely on them.
    for d in deaths.read() {
        if d.gold > 0 || d.wood > 0 {
            res.gold += d.gold;
            res.wood += d.wood;
            changed.write(snapshot(&res, &pop));
        }
    }
    for cmd in cmds.read() {
        match *cmd {
            EconCommand::Spend { gold, wood } => {
                if res.gold >= gold && res.wood >= wood {
                    res.gold -= gold;
                    res.wood -= wood;
                    changed.write(snapshot(&res, &pop));
                } else {
                    rejected.write(EconRejected { reason: format!("insufficient funds: need {gold}g/{wood}w") });
                }
            }
            EconCommand::Grant { gold, wood } => {
                res.gold += gold;
                res.wood += wood;
                changed.write(snapshot(&res, &pop));
            }
            EconCommand::ConvertWood { wood } => {
                if wood == 0 || res.wood < wood {
                    rejected.write(EconRejected { reason: format!("cannot convert {wood} wood (have {})", res.wood) });
                } else {
                    res.wood -= wood;
                    res.gold += wood * rules.wood_to_gold;
                    changed.write(snapshot(&res, &pop));
                }
            }
            EconCommand::BuyPopulation => {
                let cost = (rules.pop_cost_base as f32
                    * rules.pop_cost_growth.powi(purchases.0 as i32))
                .round() as u32;
                if res.wood < cost {
                    rejected.write(EconRejected { reason: format!("population costs {cost} wood (have {})", res.wood) });
                } else {
                    res.wood -= cost;
                    pop.cap = pop.cap.saturating_add(rules.pop_gain);
                    purchases.0 += 1;
                    changed.write(snapshot(&res, &pop));
                }
            }
            EconCommand::PopUse { delta } => {
                let wanted = pop.used as i16 + delta as i16;
                if wanted > i16::from(pop.cap) {
                    rejected.write(EconRejected { reason: "population full".into() });
                } else {
                    pop.used = wanted.clamp(0, i16::from(pop.cap)) as u8;
                    changed.write(snapshot(&res, &pop));
                }
            }
        }
    }
}
