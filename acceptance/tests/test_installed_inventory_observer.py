"""Labelled incomplete/stale installed-report controls, never candidate proof."""
import copy
import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[1] / "run-installed-executor.py"
SPEC = importlib.util.spec_from_file_location("installed_inventory_observer_fixture", PATH)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class InstalledInventoryTests(unittest.TestCase):
    def report(self, scenario="full"):
        _, names, _ = RUNNER.selection(scenario)
        return dict(verdict="passed", release_eligible=scenario == "full",
            filter=None if scenario == "full" else "inventory:baseline" if scenario == "baseline" else scenario,
            candidate=dict(source_commit="a" * 40, binary_sha256="b" * 64,
                identity_matches=True, build_provenance_verified=True),
            selected_scenarios=names, required_scenarios=RUNNER.selection("full")[1],
            scenarios=[dict(name=name, verdict="passed", assertions=[dict(passed=True)]) for name in names])

    def test_full_selects_the_entire_inventory_without_a_filter(self):
        arguments, names, cells = RUNNER.selection("full")
        self.assertEqual(arguments, [])
        self.assertEqual(len(names), 29)
        self.assertEqual(len(set(names)), 29)
        self.assertEqual(cells, 87)
        self.assertEqual(cells, sum(RUNNER.CELLS[:-1]))
        RUNNER.validate_installed_report(self.report(), "full", "a" * 40, "b" * 64)
        with self.assertRaises(ValueError):
            RUNNER.selection("unregistered")

    def test_focused_and_baseline_passes_cannot_substitute_for_full(self):
        for scenario in ("baseline", RUNNER.SCENARIOS[0]):
            with self.subTest(scenario=scenario):
                RUNNER.validate_installed_report(self.report(scenario), scenario, "a" * 40, "b" * 64)
                with self.assertRaises(ValueError):
                    RUNNER.validate_installed_report(self.report(scenario), "full", "a" * 40, "b" * 64)

    def test_full_refuses_missing_duplicate_extra_or_reordered_scenarios(self):
        original = self.report()
        for change in ("missing", "duplicate", "extra", "required", "order"):
            report = copy.deepcopy(original)
            if change == "missing": report["scenarios"].pop()
            elif change == "duplicate": report["scenarios"][-1] = report["scenarios"][0]
            elif change == "extra": report["scenarios"].append(report["scenarios"][0])
            elif change == "required": report["required_scenarios"] = report["required_scenarios"][:-1]
            else: report["scenarios"].reverse()
            with self.subTest(change=change), self.assertRaises(ValueError):
                RUNNER.validate_installed_report(report, "full", "a" * 40, "b" * 64)

    def test_full_refuses_failed_unknown_skipped_or_empty_observations(self):
        for verdict in ("failed", "unknown", "skipped", "passed"):
            report = self.report()
            report["scenarios"][0]["verdict"] = verdict
            if verdict == "passed": report["scenarios"][0]["assertions"] = []
            with self.subTest(verdict=verdict), self.assertRaises(ValueError):
                RUNNER.validate_installed_report(report, "full", "a" * 40, "b" * 64)
        for value in (False, 1, None):
            report = self.report(); report["scenarios"][0]["assertions"][0]["passed"] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                RUNNER.validate_installed_report(report, "full", "a" * 40, "b" * 64)

    def test_full_refuses_stale_unverified_or_filtered_identity(self):
        for change in (dict(source_commit="c" * 40), dict(binary_sha256="d" * 64),
                       dict(identity_matches=False), dict(build_provenance_verified=False)):
            report = self.report(); report["candidate"].update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                RUNNER.validate_installed_report(report, "full", "a" * 40, "b" * 64)
        report = self.report(); report["filter"] = "subset"
        with self.assertRaises(ValueError):
            RUNNER.validate_installed_report(report, "full", "a" * 40, "b" * 64)


if __name__ == "__main__":
    unittest.main()
