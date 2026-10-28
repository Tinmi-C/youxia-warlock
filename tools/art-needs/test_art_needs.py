"""Unit tests for art_needs pure functions (grouping, diff, templates).

Run: python3 -m unittest discover tools/art-needs -v
No network, no cargo — dump shapes are fixtures mirroring mapbundle-dump.
(The SfxKind source-sync test reads the real sfx/mod.rs but parses text
only; it still needs no cargo.)
"""
from __future__ import annotations

import unittest
from pathlib import Path

from art_needs import (
    _APPEARANCE_FIELDS,
    _SFX_USAGE,
    appearance_differs,
    default_spec,
    diff_demands,
    group_units,
    merge_manual_cards,
    parse_sfx_kinds,
    sfx_demand_cards,
    ui_demand_cards,
    unit_snapshots,
)


def unit(uid: str, kind: str, upgrade_to: str | None = None,
         wave_refs: int = 0, has_aura: bool = False) -> dict:
    return {
        "id": uid, "kind": kind, "hp": 80.0, "move_speed": 0.0,
        "body_radius": 0.4, "armor_type": "Fortified", "attack_type": None,
        "traits": [], "upgrade_to": upgrade_to, "has_aura": has_aura,
        "hero": kind == "hero", "wave_refs": wave_refs,
    }


class GroupUnitsTest(unittest.TestCase):
    def test_upgrade_chain_merges_into_one_demand(self):
        units = [
            unit("arrow", "tower", upgrade_to="arrow2"),
            unit("arrow2", "tower", upgrade_to="arrow3"),
            unit("arrow3", "tower"),
        ]
        groups = group_units(units)
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0]["local_key"], "tower-arrow")
        self.assertEqual(groups[0]["source_defs"], ["arrow", "arrow2", "arrow3"])
        self.assertEqual(groups[0]["meta"]["tiers"], 3)

    def test_chain_members_are_consumed(self):
        units = [
            unit("arrow", "tower", upgrade_to="arrow2"),
            unit("arrow2", "tower"),
            unit("grunt", "creep", wave_refs=30),
        ]
        groups = group_units(units)
        keys = [g["local_key"] for g in groups]
        self.assertEqual(keys, ["creep-grunt", "tower-arrow"])

    def test_cycle_guard_terminates(self):
        # A malformed cycle must not hang the scan (loader rejects it first,
        # but the tool must stay defensive).
        units = [
            unit("a", "tower", upgrade_to="b"),
            unit("b", "tower", upgrade_to="a"),
        ]
        groups = group_units(units)
        self.assertEqual(len(groups), 1)  # both consumed as one group

    def test_kind_naming_and_sort(self):
        units = [
            unit("tank", "creep", wave_refs=60),
            unit("hunter", "hero"),
            unit("ice", "tower", has_aura=True),
        ]
        groups = group_units(units)
        self.assertEqual([g["local_key"] for g in groups],
                         ["creep-tank", "hero-hunter", "tower-ice"])
        self.assertTrue(groups[0]["meta"]["wave_refs"], 60)
        self.assertTrue(groups[2]["meta"]["has_aura"])


class DiffDemandsTest(unittest.TestCase):
    def test_new_group_detected(self):
        existing = [{"local_key": "creep-grunt", "source_defs": ["grunt"],
                     "demand_id": "abc", "status": "submitted"}]
        groups = group_units([unit("grunt", "creep"), unit("arrow", "tower")])
        d = diff_demands(existing, groups)
        self.assertEqual([g["local_key"] for g in d["new"]], ["tower-arrow"])
        self.assertEqual(d["removed"], [])
        self.assertEqual(d["changed"], [])

    def test_removed_when_def_disappears(self):
        existing = [{"local_key": "creep-grunt", "source_defs": ["grunt"],
                     "demand_id": "abc", "status": "submitted"}]
        d = diff_demands(existing, group_units([unit("arrow", "tower")]))
        self.assertEqual(d["removed"], ["creep-grunt"])

    def test_chain_growth_is_change_not_new(self):
        # arrow grows a tier: same chain head, def set drifts.
        existing = [{"local_key": "tower-arrow",
                     "source_defs": ["arrow", "arrow2"],
                     "demand_id": "abc", "status": "submitted"}]
        units = [
            unit("arrow", "tower", upgrade_to="arrow2"),
            unit("arrow2", "tower", upgrade_to="arrow3"),
            unit("arrow3", "tower"),
        ]
        d = diff_demands(existing, group_units(units))
        self.assertEqual(d["new"], [])
        self.assertEqual(d["changed"], ["tower-arrow"])

    def test_removed_entries_are_ignored(self):
        existing = [{"local_key": "tower-ice", "source_defs": ["ice"],
                     "removed": True, "demand_id": None, "status": "draft"}]
        d = diff_demands(existing, group_units([unit("ice", "tower")]))
        # Not re-added: a human decided this def needs no demand.
        self.assertEqual(d["new"], [])
        self.assertEqual(d["changed"], [])


class TemplatesTest(unittest.TestCase):
    def test_tower_chain_gets_variants(self):
        g = {"local_key": "tower-arrow", "kind": "tower",
             "source_defs": ["arrow", "arrow2", "arrow3"],
             "meta": {"tiers": 3, "has_aura": False, "wave_refs": 0}}
        spec = default_spec(g)
        self.assertEqual(spec["variants"], ["arrow (tier 1)", "arrow2 (tier 2)",
                                            "arrow3 (tier 3)"])

    def test_heavy_rotation_creep_budget_tightened(self):
        g = {"local_key": "creep-grunt", "kind": "creep", "source_defs": ["grunt"],
             "meta": {"tiers": 1, "has_aura": False, "wave_refs": 120}}
        spec = default_spec(g)
        self.assertIn("压预算", spec["poly_budget"])


# --- round 4 NEEDS: UI declaration manifest -> manual_ui cards ------------

def ui_item(uid: str, kind: str = "icon") -> dict:
    return {"id": uid, "kind": kind, "size_px": 128 if kind == "icon" else [416, 256],
            "usage": f"{uid} 用途", "acceptance": "紧裁剪双帧对比 + token 色差容差"}


class UiManifestTest(unittest.TestCase):
    def test_ui_cards_are_manual_lane(self):
        cards = ui_demand_cards([ui_item("panel-hud", "panel"),
                                 ui_item("icon-gold")])
        self.assertEqual([c["local_key"] for c in cards],
                         ["ui-panel-hud", "ui-icon-gold"])
        for c in cards:
            self.assertEqual(c["origin"], "manual_ui")
            self.assertEqual(c["category"], "ui")
            self.assertEqual(c["source_defs"], [])  # manual lane: diff never touches
            self.assertEqual(c["priority"], "P0")
        self.assertIn("切分", cards[0]["spec"]["nine_slice"])   # panel spec
        self.assertIn("栅格", cards[1]["spec"]["grid"])          # icon spec

    def test_merge_creates_once_then_never_touches(self):
        items = [ui_item("icon-gold")]
        first_pass, kept = merge_manual_cards([], ui_demand_cards(items))
        self.assertEqual((len(first_pass), kept), (1, 0))
        demands = list(first_pass)
        demands[0]["name"] = "人工改过的名字"  # human refinement after creation
        second_pass, kept2 = merge_manual_cards(demands, ui_demand_cards(items))
        self.assertEqual((len(second_pass), kept2), (0, 1))
        self.assertEqual(demands[0]["name"], "人工改过的名字")  # untouched

    def test_manual_ui_entry_never_removed_by_diff(self):
        # The manual-free-delete lane: no source_defs -> diff can neither
        # remove nor change it, even against an empty scan.
        existing = [{"local_key": "ui-icon-gold", "origin": "manual_ui",
                     "source_defs": [], "demand_id": None, "status": "draft"}]
        d = diff_demands(existing, group_units([unit("grunt", "creep")]))
        self.assertEqual(d["removed"], [])
        self.assertEqual(d["changed"], [])


# --- round 4 NEEDS: SfxKind enum -> manual audio cards ---------------------

SFX_SOURCE_FIXTURE = """\
pub enum SfxKind {
    ClickAck,
    Select,
    /// rule::MatchEnded — win or lose sting (AUD1 addition).
    MatchEnd,
}

#[derive(Message)]
pub struct SfxRequest {
    pub kind: SfxKind,
}
"""


class SfxProjectionTest(unittest.TestCase):
    def test_parse_sfx_kinds_from_source(self):
        variants = parse_sfx_kinds(SFX_SOURCE_FIXTURE)
        self.assertEqual(variants, ["ClickAck", "Select", "MatchEnd"])

    def test_sfx_cards_shape(self):
        cards = sfx_demand_cards(["Hit", "WaveStart"])
        self.assertEqual([c["local_key"] for c in cards],
                         ["sfx-hit", "sfx-wave_start"])
        for c in cards:
            self.assertEqual(c["origin"], "sfx")
            self.assertEqual(c["category"], "audio")
            self.assertEqual(c["source_defs"], [])
        self.assertIn("命中", cards[0]["name"])  # usage semantics in the name
        self.assertEqual(cards[0]["spec"]["format"], "ogg")

    def test_real_enum_and_usage_table_stay_in_sync(self):
        # Anti-drift assert: parse the REAL engine enum. If a variant was
        # added, the count assertion fails; if it lacks a Chinese usage
        # mapping, the unmapped check fails — update _SFX_USAGE either way.
        # (Round-4 plan said "14 variants"; the source at round-4 NEEDS time
        # has 13 — the parse keeps the projection honest either way.)
        src = (Path(__file__).resolve().parents[2]
               / "engine/warbase/src/sfx/mod.rs").read_text(encoding="utf-8")
        variants = parse_sfx_kinds(src)
        self.assertEqual(len(variants), 13)
        self.assertEqual([v for v in variants if v not in _SFX_USAGE], [])


# --- round 4 NEEDS: appearance-relevance filter -----------------------------

class AppearanceFilterTest(unittest.TestCase):
    def setUp(self):
        # Baseline world: grunt with default fixture fields.
        self.old_units = [unit("grunt", "creep")]
        self.snapshots = unit_snapshots(self.old_units,
                                        group_units(self.old_units))
        self.existing = [{"local_key": "creep-grunt", "origin": "scan",
                          "source_defs": ["grunt"], "demand_id": None,
                          "status": "draft"}]

    def diff(self, new_units: list[dict], strict: bool = False) -> dict:
        return diff_demands(self.existing, group_units(new_units),
                            snapshots=self.snapshots,
                            scanned_units=new_units, strict=strict)

    def test_hp_change_is_silent(self):
        d = self.diff([{**unit("grunt", "creep"), "hp": 999.0}])
        self.assertEqual(d["changed"], [])
        self.assertEqual(d["silent"], ["creep-grunt"])

    def test_body_radius_change_is_changed(self):
        d = self.diff([{**unit("grunt", "creep"), "body_radius": 0.6}])
        self.assertEqual(d["changed"], ["creep-grunt"])
        self.assertEqual(d["silent"], [])

    def test_armor_type_and_traits_are_appearance(self):
        d = self.diff([{**unit("grunt", "creep"), "armor_type": "Light"}])
        self.assertEqual(d["changed"], ["creep-grunt"])
        d = self.diff([{**unit("grunt", "creep"),
                        "traits": ["Regenerate"]}])
        self.assertEqual(d["changed"], ["creep-grunt"])

    def test_strict_counts_every_field(self):
        d = self.diff([{**unit("grunt", "creep"), "hp": 999.0}], strict=True)
        self.assertEqual(d["changed"], ["creep-grunt"])

    def test_no_baseline_means_def_set_drift_only(self):
        # Entries predating the filter have no snapshot: pure field drift
        # (even appearance fields) stays unreported — only def-set drift
        # marks changed. The write path backfills the baseline.
        new_units = [{**unit("grunt", "creep"), "body_radius": 0.9,
                      "hp": 1.0}]
        d = diff_demands(self.existing, group_units(new_units),
                         scanned_units=new_units)
        self.assertEqual(d["changed"], [])
        self.assertEqual(d["silent"], [])

    def test_appearance_differs_unit(self):
        old = unit("grunt", "creep")
        self.assertFalse(appearance_differs(old, {**old, "hp": 5.0}))
        self.assertTrue(appearance_differs(old, {**old, "hp": 5.0}, strict=True))
        self.assertTrue(appearance_differs(old, None))  # def vanished
        self.assertIn("body_radius", _APPEARANCE_FIELDS)


if __name__ == "__main__":
    unittest.main()
