"""Injected clocks/resources are harness controls, never installed soak proof."""
import copy
import json
from pathlib import Path
import unittest

from acceptance.suite.soak import CASES, NANOSECONDS, RunState, schedule, validate_profile
from acceptance.suite.metrics import METRICS, GROWTH, calibration_for, validate_sample

PROFILE_PATH = Path(__file__).resolve().parents[1] / 'profiles/operational.json'


class Clock:
    value = 0

    def __call__(self):
        return self.value * NANOSECONDS


class CompletionTests(unittest.TestCase):
    def profile(self, name='smoke'):
        return json.loads(PROFILE_PATH.read_text())['profiles'][name]

    def complete(self, state, count):
        for entry in schedule(123, count):
            state.complete(entry)

    def test_neither_duration_nor_cycle_count_alone_passes(self):
        for name, count in (('smoke', 100), ('sustained', 10_000)):
            clock = Clock(); state = RunState(self.profile(name), clock)
            self.complete(state, count)
            self.assertEqual(state.verdict(), 'running')
        clock = Clock(); state = RunState(self.profile(), clock)
        for entry in schedule(123, 99):
            clock.value += 2; state.complete(entry)
        self.assertEqual(state.verdict(), 'running')
        state.complete(dict(index=99, case='success'))
        self.assertEqual(state.verdict(), 'passed')

    def test_sustained_cannot_finish_before_eight_hours_of_actual_completed_work(self):
        clock = Clock(); state = RunState(self.profile('sustained'), clock)
        self.complete(state, 10_000)
        for entry in list(schedule(321, 500))[1:]:
            clock.value += 60
            state.complete(dict(index=state.completed, case=entry['case']))
            if clock.value < 28_800:
                self.assertEqual(state.verdict(), 'running')
            else:
                self.assertEqual(state.verdict(), 'passed'); break

    def test_idle_tail_cannot_pad_duration_even_before_watchdog_expiry(self):
        clock = Clock(); state = RunState(self.profile(), clock)
        clock.value = 40; self.complete(state, 100)
        clock.value = 120
        self.assertEqual(state.verdict(), 'running')
        state.complete(dict(index=100, case='success'))
        self.assertEqual(state.verdict(), 'passed')

    def test_no_progress_maximum_time_and_clock_regression_are_incomplete(self):
        for time, reason in ((90, 'no_progress'), (1800, 'maximum_duration'), (-1, None)):
            clock = Clock(); state = RunState(self.profile(), clock); clock.value = time
            if reason is None:
                with self.assertRaises(ValueError): state.verdict()
            else:
                self.assertEqual(state.verdict(), 'incomplete'); self.assertEqual(state.reason, reason)
        clock = Clock(); clock.value = 10; state = RunState(self.profile(), clock)
        clock.value = 9; self.assertEqual(state.verdict(), 'incomplete')
        clock.value = 10; self.assertEqual(state.verdict(), 'incomplete')

    def test_missing_duplicate_or_foreign_cycles_do_not_advance(self):
        for entry in (dict(index=1, case='success'), dict(index=True, case='success'),
                      dict(index=0, case='unknown')):
            state = RunState(self.profile(), Clock())
            with self.assertRaises(ValueError): state.complete(entry)
            self.assertEqual(state.completed, 0); self.assertEqual(state.verdict(), 'incomplete')

    def test_all_declared_faults_are_required_and_terminal_runs_cannot_continue(self):
        clock = Clock(); state = RunState(self.profile(), clock)
        for index in range(100):
            clock.value += 2; state.complete(dict(index=index, case='success'))
        self.assertEqual(state.verdict(), 'running')
        for case in CASES[1:]: state.complete(dict(index=state.completed, case=case))
        self.assertEqual(state.verdict(), 'passed')
        with self.assertRaises(ValueError): state.complete(dict(index=state.completed, case='success'))

    def test_count_ceiling_does_not_turn_early_completion_into_duration_proof(self):
        profile = self.profile(); profile['maximum_cycles'] = 100
        state = RunState(profile, Clock()); self.complete(state, 100)
        self.assertEqual(state.verdict(), 'incomplete')
        self.assertEqual(state.reason, 'maximum_cycles_before_duration')

    def test_profiles_refuse_shortened_floors_and_invalid_numeric_bounds(self):
        for name, changes in (('smoke', {'minimum_seconds':119}), ('smoke', {'minimum_cycles':99}),
            ('sustained', {'minimum_seconds':28799}), ('sustained', {'minimum_cycles':9999}),
            ('smoke', {'watchdog_seconds':0}), ('smoke', {'maximum_seconds':120}),
            ('smoke', {'minimum_cycles':True})):
            profile = self.profile(name); profile.update(changes)
            with self.assertRaises(ValueError): validate_profile(profile)

    def test_seed_replays_fault_order_without_an_os_clock(self):
        first = list(schedule(123, 120)); self.assertEqual(first, list(schedule(123, 120)))
        self.assertNotEqual(first, list(schedule(124, 120)))
        self.assertEqual({row['index'] for row in first}, set(range(120)))
        for start in range(0, 120, 12): self.assertEqual({row['case'] for row in first[start:start+12]}, set(CASES))
        self.assertEqual({(row['handler_kind'], row['transport']) for row in first},
            {(kind, transport) for kind in ('process','mcp') for transport in ('standalone','stdio','http')})
        self.assertEqual({(row['case'], row['handler_kind'], row['transport']) for row in first[:72]},
            {(case, kind, transport) for case in CASES for kind in ('process','mcp')
             for transport in ('standalone','stdio','http')})
        for start in range(0, 120, 12):
            self.assertEqual(len({(row['handler_kind'], row['transport'])
                                  for row in first[start:start+12]}), 1)


class ResourceTests(unittest.TestCase):
    def calibration(self):
        # Explicit synthetic calibration; no named native host is calibrated here.
        return dict(status='calibrated', workload_sha256='a'*64, evidence_sha256='b'*64,
            ceilings={key:1000 for key in METRICS}, quiescent_growth={key:10 for key in GROWTH})

    def sample(self, phase='quiescent'):
        values={key:100 for key in METRICS}
        if phase=='quiescent':
            for key in ('active_children','surviving_descendants','capture_bytes'): values[key]=0
        return dict(phase=phase, metrics=values, equivalent_state=dict(live_hosts=1, handled_pending=0))

    def test_missing_uncalibrated_stale_or_partial_host_budget_cannot_pass(self):
        for calibration in (None, {}, {**self.calibration(),'status':'required'},
            {**self.calibration(),'workload_sha256':'c'*64}, {**self.calibration(),'ceilings':{}},
            {**self.calibration(),'evidence_sha256':'unknown'}):
            with self.assertRaises(ValueError): calibration_for('synthetic', {'synthetic':calibration}, 'a'*64)
        self.assertEqual(calibration_for('synthetic', {'synthetic':self.calibration()}, 'a'*64), self.calibration())
        self.assertIsInstance(json.loads(PROFILE_PATH.read_text())['calibrations'], dict)

    def test_each_unavailable_nonnumeric_or_over_ceiling_metric_refuses(self):
        for metric in METRICS:
            for value in (None, True, -1, float('nan'), 1001):
                sample=self.sample('active');sample['metrics'][metric]=value
                with self.subTest(metric=metric,value=value), self.assertRaises(ValueError):validate_sample(sample,self.calibration())
            sample=self.sample();del sample['metrics'][metric]
            with self.assertRaises(ValueError):validate_sample(sample,self.calibration())

    def test_retired_children_descendants_and_capture_bytes_are_exact_zero(self):
        for metric in ('active_children','surviving_descendants','capture_bytes'):
            sample=self.sample();sample['metrics'][metric]=1
            with self.assertRaises(ValueError):validate_sample(sample,self.calibration())

    def test_growth_requires_valid_equivalent_warmed_states_and_separates_journal_growth(self):
        warmed=self.sample();sample=self.sample();sample['metrics']['journal_bytes']=900
        self.assertEqual(validate_sample(sample,self.calibration(),warmed),sample)
        for metric in ('rss_bytes','file_descriptors'):
            altered=copy.deepcopy(sample);altered['metrics'][metric]+=11
            with self.assertRaises(ValueError):validate_sample(altered,self.calibration(),warmed)
        altered=copy.deepcopy(sample);altered['equivalent_state']['live_hosts']=2
        with self.assertRaises(ValueError):validate_sample(altered,self.calibration(),warmed)
        warmed['metrics']['rss_bytes']=True
        with self.assertRaises(ValueError):validate_sample(sample,self.calibration(),warmed)


if __name__ == '__main__':
    unittest.main()
