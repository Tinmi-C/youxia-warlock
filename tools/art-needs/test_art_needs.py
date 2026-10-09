"""Unit tests for art_needs pure functions (grouping, diff, templates).

Run: python3 -m unittest discover tools/art-needs -v
No network, no cargo — dump shapes are fixtures mirroring mapbundle-dump.
"""
from __future__ import annotations

import unittest

from art_needs import default_spec, diff_demands, group_units


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


if __name__ == "__main__":
    unittest.main()
