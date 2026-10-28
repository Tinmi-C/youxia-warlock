//! Economy (BX7): gold/wood/population through a single command stream —
//! same single-writer idea as the damage pipeline. Kill bounties flow in
//! here too (UnitDied -> Grant), so readings and cheat panels come free.
//!
//! L2 (round-4 LDEBT): per-player `Wallets` + `LocalPlayer`. The legacy
//! `Resources`/`Population` singletons stay as the LOCAL player's view
//! (every pre-MP consumer — build/item/production/harvest/game HUDs —
//! keeps its signature and behavior); [`sync_local_wallet`] reconciles
//! the two sides with a value watermark (minimal writes, see its docs).

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

// ---------------------------------------------------------------------------
// L2 (round-4 LDEBT): per-player economy
// ---------------------------------------------------------------------------

/// One player's full economy account. `pop_used`/`pop_cap` live here so
/// [`Population`] can be a pure local-player view of [`Wallets`] (the same
/// reconciliation as gold/wood — one account shape, one sync).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Wallet {
    pub gold: u32,
    pub wood: u32,
    pub pop_used: u8,
    pub pop_cap: u8,
}

/// Per-player wallets, index = player slot (`PlayerId.0`); seeded by
/// `assemble` from `econ_start` for every listed player. MP games drive
/// these directly (e.g. legion's hire/send/income loop); single-player
/// games never touch them and stay on the `Resources` view.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Wallets(pub Vec<Wallet>);

impl Wallets {
    /// Wallet of `player`, growing the vec with zeroed wallets when the
    /// slot is beyond the seeded range (defensive; `assemble` covers every
    /// listed player).
    pub fn of_mut(&mut self, player: crate::unit::PlayerId) -> &mut Wallet {
        let i = player.0 as usize;
        if i >= self.0.len() {
            self.0.resize(i + 1, Wallet::default());
        }
        &mut self.0[i]
    }
}

/// Which slot the local human/UI represents (no netcode; default P0).
/// Decides which wallet the legacy `Resources`/`Population` singletons
/// mirror, and where un-attributed income lands.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalPlayer(pub crate::unit::PlayerId);

impl Default for LocalPlayer {
    fn default() -> Self {
        Self(crate::unit::PlayerId(0))
    }
}

/// ENGINE-INTERNAL watermark: the account value both sides (`Wallets[local]`
/// and `Resources`/`Population`) last agreed on. Written only by
/// [`sync_local_wallet`]; nobody else reads or writes it.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LocalWalletMirror(pub Wallet);

/// How many population purchases happened (drives the cost curve).
#[derive(Resource, Debug, Default)]
pub struct PopPurchases(pub u32);

/// Player-facing econ commands. `Serialize, Deserialize` added by RPL1
/// (authorized derive-only change) so the replay module can store them in
/// `CommandEnvelope::Ext` payloads; behavior is untouched.
#[derive(Message, Debug, Serialize, Deserialize)]
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

/// Econ pipeline. L2 changes ONLY the bounty arm: kill rewards are
/// attributed per player — the KILLER's `Owner` wallet is credited; a
/// killer without `Owner` (every tower/creep on single-player maps) folds
/// to `LocalPlayer`, which is exactly the pre-L2 global-`Resources`
/// semantics (zero drift). When the credited player IS the local one, the
/// legacy view is written too so pre-MP consumers observe the bounty the
/// same frame they always did.
///
/// L6 SCHEDULING CONTRACT (pinned here because this system now derefs
/// `UnitDied.killer`): `killer` is an Entity reference and
/// `combat::cleanup_dead` despawns Dead-marked bodies (including a mutual
/// kill's killer) later in the SAME Cleanup set. Any consumer resolving
/// killer attribution through the world (Owner lookups etc.) MUST run
/// `in_set(WarbaseSet::Cleanup).before(cleanup_dead)` — or, like this
/// system, in `WarbaseSet::Tick`, which is ordered before Cleanup. Running
/// after `cleanup_dead` means `owners.get(killer)` fails and the bounty
/// silently drops.
pub fn apply_econ(
    mut cmds: MessageReader<EconCommand>,
    mut deaths: MessageReader<crate::combat::UnitDied>,
    mut res: ResMut<Resources>,
    mut pop: ResMut<Population>,
    mut purchases: ResMut<PopPurchases>,
    rules: Res<EconRules>,
    local: Option<Res<LocalPlayer>>,
    mut wallets: Option<ResMut<Wallets>>,
    owners: Query<&crate::unit::Owner>,
    mut changed: MessageWriter<EconChanged>,
    mut rejected: MessageWriter<EconRejected>,
) {
    // L2 params are Option-al on purpose: consumers that cherry-pick this
    // system WITHOUT WarbasePlugin (warden wires it into its own schedule)
    // never registered the per-player resources — absent wallets = pure
    // pre-L2 semantics (global Resources credit), zero drift for them.
    let local = local.map(|l| l.0).unwrap_or(crate::unit::PlayerId(0));
    // Kill bounties first so a same-frame Spend can rely on them.
    for d in deaths.read() {
        if d.gold > 0 || d.wood > 0 {
            // L2: bounty attribution — the killer's owner, else local.
            let credited = match d.killer {
                Some(crate::combat::DamageOrigin::Attack(e))
                | Some(crate::combat::DamageOrigin::Skill { caster: e, .. }) => {
                    owners.get(e).map(|o| o.0).unwrap_or(local)
                }
                _ => local,
            };
            if let Some(w) = wallets.as_deref_mut() {
                let w = w.of_mut(credited);
                w.gold += d.gold;
                w.wood += d.wood;
            }
            if wallets.is_none() || credited == local {
                // Legacy local view: identical to the pre-L2 write (and the
                // ONLY write when per-player wallets are not in play).
                res.gold += d.gold;
                res.wood += d.wood;
            }
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

/// L2 two-way reconciliation between `Wallets[local]` and the legacy
/// `Resources`/`Population` singletons, scheduled once per frame at the end
/// of `WarbaseSet::Cleanup` (after `apply_econ` in Tick and most game
/// writes; games writing even later are caught next frame — values never
/// regress, only the mirror lags).
///
/// MINIMAL-WRITE ALGORITHM (value watermark, no change-tick tricks):
/// `LocalWalletMirror` is the account both sides last agreed on. Per run:
/// - wallet side unchanged, view side changed  => ABSORB: the legacy direct
///   writers (build/item/production/harvest systems, game HUDs and cheats —
///   all local-player semantics) flow INTO the wallet. Single-player games
///   (warden/loop-td/rts-lab) only ever take this branch: bit-identical
///   behavior, one extra struct copy per actual change.
/// - view side unchanged, wallet side changed  => PROJECT: MP games driving
///   `Wallets` per player (legion's hire/send/income) flow INTO the view.
/// - neither changed                           => ZERO writes (idle frames
///   cost 12 integer compares).
/// - both changed                              => the WALLET wins: the view
///   is defined "synced FROM wallets". A frame where the same account is
///   written through both sides is a single-writer-convention violation
///   (pick one side per player account); wallet-wins keeps engine-internal
///   double-writes (`apply_econ` credits the wallet AND the local view for
///   a local kill, then a game wallet spend in the same window) coherent,
///   because the engine never relies on its own view write surviving.
pub fn sync_local_wallet(
    local: Res<LocalPlayer>,
    mut wallets: ResMut<Wallets>,
    mut mirror: ResMut<LocalWalletMirror>,
    mut res: ResMut<Resources>,
    mut pop: ResMut<Population>,
) {
    let wallet = *wallets.of_mut(local.0);
    let view = Wallet { gold: res.gold, wood: res.wood, pop_used: pop.used, pop_cap: pop.cap };
    if wallet == view {
        // Converged (also the every-frame case once synced): no writes.
        mirror.0 = view;
        return;
    }
    let (next, absorb) = if wallet == mirror.0 {
        (view, true) // only the legacy view moved -> absorb into the wallet
    } else {
        (wallet, false) // wallet moved (or both) -> wallet wins, project
    };
    mirror.0 = next;
    if absorb {
        *wallets.of_mut(local.0) = next;
    } else {
        res.gold = next.gold;
        res.wood = next.wood;
        pop.used = next.pop_used;
        pop.cap = next.pop_cap;
    }
}
