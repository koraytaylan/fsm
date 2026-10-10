"""Auxiliary real fixture/procfs controls, never installed native soak proof."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

from acceptance.suite.fsm import Scratch
from acceptance.suite.soak_resources import observe_live_host
from acceptance.suite.soak_workload import observe_completed_cycle

FIXTURE = Path(__file__).resolve().parents[1] / 'fixtures/executor_handler.py'
SUCCESS = dict(resource='supplier',
    attempts=[('validate', 0), ('suspend', 0), ('process', 0), ('restore', 0)],
    mutations=['suspend', 'process:0', 'process:1', 'restore'],
    state={'suspended': False, 'items': [0, 1]})


def observations(root):
    rows = {name: [json.loads(line) for line in (root / (name + '.jsonl')).read_text().splitlines()]
            for name in ('entries', 'results', 'timings', 'trace')}
    rows['state'] = json.loads((root / (hashlib.sha256(b'supplier').hexdigest() + '.json')).read_text())
    return rows


@unittest.skipUnless(sys.platform == 'linux', 'physical cycle birth observations require Linux')
class PhysicalCycleTests(unittest.TestCase):
    def invoke(self, root, operation, *flags, code=0, mode='operation', label='cycle'):
        command = [sys.executable, str(FIXTURE), mode, '--root', str(root), '--run', label,
                   '--resource', 'supplier', '--observe-timing', '--delay-ms', '1', *flags]
        frames = None
        if mode == 'operation':
            command += ['--operation', operation]
        else:
            frames = '\n'.join(json.dumps(row) for row in (
                dict(jsonrpc='2.0', id=1, method='initialize', params={'protocolVersion':'2025-06-18'}),
                dict(jsonrpc='2.0', method='notifications/initialized'),
                dict(jsonrpc='2.0', id=2, method='tools/call',
                     params={'name':'operate', 'arguments':{'operation':operation}}))) + '\n'
        result = subprocess.run(command, input=frames, text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, code if mode == 'operation' else 0, result.stderr)
        if mode == 'mcp':
            response = json.loads(result.stdout.splitlines()[-1])['result']
            self.assertEqual(response['structuredContent']['exit_code'], code)
            self.assertEqual(response['isError'], code != 0)

    def cycle(self, root, mode='operation'):
        started = time.monotonic_ns()
        for operation, _ in SUCCESS['attempts']:
            self.invoke(root, operation, mode=mode)
        completed = time.monotonic_ns()
        return started, completed, observations(root)

    def test_real_process_and_mcp_cycles_bind_independent_timings_and_mutations(self):
        for mode in ('operation', 'mcp'):
            with self.subTest(mode=mode), Scratch('ordinary-soak-cycle') as scratch:
                started, completed, rows = self.cycle(Path(scratch.path), mode)
                result = observe_completed_cycle(SUCCESS, rows, started, completed)
                self.assertEqual(len(result['physical_runs']), 4)
                self.assertGreater(result['work_latency_ns'], result['scheduler_lag_ns'])
                self.assertTrue(all(end > start for _, start, end in result['intervals']))
                self.assertEqual(set(rows['entries'][0]), {'run','resource','operation','pid','pid_starttime'})

    def test_fail_first_is_one_failed_entry_then_success_for_each_explicit_cycle_label(self):
        for mode in ('operation', 'mcp'):
            with self.subTest(mode=mode), Scratch('ordinary-soak-retry') as scratch:
                root = Path(scratch.path)
                for label in ('cycle', 'cycle:second'):
                    self.invoke(root, 'validate', '--fail-first', mode=mode, code=3, label=label)
                    self.invoke(root, 'validate', '--fail-first', mode=mode, label=label)
                rows = [json.loads(line) for line in (root/'results.jsonl').read_text().splitlines()]
                self.assertEqual([row['exit_code'] for row in rows], [3, 0, 3, 0])
                self.assertEqual(len({row['run'] for row in rows}), 4)

    def test_real_failed_attempt_then_retry_completes_the_authored_cycle(self):
        for mode in ('operation', 'mcp'):
            with self.subTest(mode=mode), Scratch('ordinary-soak-completed-retry') as scratch:
                root = Path(scratch.path); started = time.monotonic_ns()
                self.invoke(root, 'validate', '--fail-first', code=3, mode=mode)
                self.invoke(root, 'validate', '--fail-first', mode=mode)
                for operation in ('suspend', 'process', 'restore'): self.invoke(root, operation, mode=mode)
                expected = copy.deepcopy(SUCCESS); expected['attempts'].insert(0, ('validate', 3))
                result = observe_completed_cycle(expected, observations(root), started, time.monotonic_ns())
                self.assertEqual(len(result['physical_runs']), 5)

    def test_real_partial_work_requires_observed_compensation_and_honest_remaining_items(self):
        with Scratch('ordinary-soak-compensation') as scratch:
            root = Path(scratch.path); started = time.monotonic_ns()
            self.invoke(root, 'validate'); self.invoke(root, 'suspend')
            self.invoke(root, 'process', '--failure', 'partial', code=3)
            self.invoke(root, 'restore')
            expected = copy.deepcopy(SUCCESS); expected['attempts'][2] = ('process', 3)
            expected['mutations'] = ['suspend', 'process:0', 'restore']; expected['state']['items'] = [0]
            self.assertEqual(len(observe_completed_cycle(expected, observations(root),
                started, time.monotonic_ns())['physical_runs']), 4)

    def test_repeated_cycles_use_the_same_resource_without_erasing_physical_history(self):
        with Scratch('ordinary-soak-repeated-resource') as scratch:
            root = Path(scratch.path)
            _, _, first = self.cycle(root)
            started, completed, both = self.cycle(root)
            second = {key: both[key][len(first[key]):] for key in ('entries','results','timings','trace')}
            second['state'] = both['state']
            result = observe_completed_cycle(SUCCESS, second, started, completed)
            self.assertEqual(len(both['entries']), 8)
            self.assertTrue(set(result['physical_runs']).isdisjoint(row['run'] for row in first['entries']))
            self.assertGreater(second['trace'][0]['seq'], first['trace'][-1]['seq'])

    def test_lost_work_changed_result_or_state_and_stuck_timing_never_complete(self):
        with Scratch('ordinary-soak-refusals') as scratch:
            started, completed, rows = self.cycle(Path(scratch.path))
            original = copy.deepcopy(rows)
            changes = []
            missing = copy.deepcopy(rows); missing['entries'].pop(); changes.append(missing)
            stuck = copy.deepcopy(rows); stuck['timings'].pop(); changes.append(stuck)
            changed = copy.deepcopy(rows); changed['timings'][-1]['exit_code'] = 3; changes.append(changed)
            changed = copy.deepcopy(rows); changed['state']['items'] = [0]; changes.append(changed)
            changed = copy.deepcopy(rows); changed['state']['suspended'] = 0; changes.append(changed)
            changed = copy.deepcopy(rows); changed['results'][0]['exit_code'] = True; changes.append(changed)
            changed = copy.deepcopy(rows); changed['timings'][0]['pid'] = True; changes.append(changed)
            changed = copy.deepcopy(rows); changed['timings'][0]['run'] = []; changes.append(changed)
            changed = copy.deepcopy(rows); changed['entries'][0]['operation'] = []; changes.append(changed)
            for changed in changes:
                with self.assertRaises((ValueError, AssertionError)):
                    observe_completed_cycle(SUCCESS, changed, started, completed)
            self.assertEqual(rows, original)

    def test_changed_birth_identity_and_physical_overlap_refuse_even_when_trace_passes(self):
        with Scratch('ordinary-soak-overlap') as scratch:
            started, completed, rows = self.cycle(Path(scratch.path))
            changed = copy.deepcopy(rows); changed['timings'][0]['pid_starttime'] += '0'
            with self.assertRaisesRegex(ValueError, 'identity'):
                observe_completed_cycle(SUCCESS, changed, started, completed)
            changed = copy.deepcopy(rows)
            changed['timings'][2]['monotonic_ns'] = changed['timings'][0]['monotonic_ns']
            changed['timings'].sort(key=lambda row: row['monotonic_ns'])
            with self.assertRaisesRegex(ValueError, 'overlaps'):
                observe_completed_cycle(SUCCESS, changed, started, completed)

    def test_bounded_fixture_inputs_refuse_before_physical_entry(self):
        with Scratch('ordinary-soak-bounds') as scratch:
            root = Path(scratch.path)
            for flags in (['--delay-ms','-1'], ['--delay-ms','2001'],
                          ['--fail-first','--failure','before']):
                result = subprocess.run([sys.executable, str(FIXTURE), 'operation', '--root', str(root),
                    '--run','bounds','--resource','supplier','--operation','validate', *flags],
                    capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 2)
                self.assertFalse((root/'entries.jsonl').exists())


class HostObservationTests(unittest.TestCase):
    def proc_fixture(self, root):
        host = root/'123'; (host/'fd').mkdir(parents=True); (host/'task'/'123').mkdir(parents=True)
        (host/'task'/'124').mkdir()
        for descriptor in ('0','1','2'): (host/'fd'/descriptor).write_text('synthetic descriptor')
        fields = ['0']*22; fields[0]='S'; fields[11]='7'; fields[12]='3'; fields[19]='456'
        (host/'stat').write_text('123 (synthetic host) '+' '.join(fields))
        (host/'status').write_text('VmRSS:\t32 kB\n')
        (host/'task'/'123'/'children').write_text('789 ')
        (host/'task'/'124'/'children').write_text('790 ')
        return host

    def test_host_counters_include_children_of_every_thread_and_retain_original_bytes(self):
        with Scratch('synthetic-soak-proc') as scratch:
            root = Path(scratch.path); self.proc_fixture(root)
            with patch('acceptance.suite.soak_resources.os.sysconf', return_value=100):
                sample = observe_live_host({'pid':123,'pid_starttime':'456'}, root)
            self.assertEqual(sample['metrics'], dict(rss_bytes=32768, file_descriptors=3,
                active_children=2, cpu_ns=100_000_000))
            self.assertEqual(sample['thread_children'], {'123':[789], '124':[790]})
            self.assertEqual(sample['stat_before'], sample['stat_after'])
            self.assertNotIn('capture_bytes', sample['metrics'])

    def test_unavailable_required_proc_metric_and_reused_or_dead_host_refuse(self):
        with Scratch('synthetic-soak-proc-refusals') as scratch:
            root = Path(scratch.path); host = self.proc_fixture(root)
            for replacement in ('VmSize: 32 kB\n', 'VmRSS: unavailable kB\n', 'VmRSS: 32 kB\nVmRSS: 32 kB\n'):
                (host/'status').write_text(replacement)
                with self.assertRaises(ValueError): observe_live_host({'pid':123,'pid_starttime':'456'}, root)
            (host/'status').write_text('VmRSS: 32 kB\n')
            with self.assertRaises(ValueError): observe_live_host({'pid':123,'pid_starttime':'457'}, root)
            (host/'stat').write_text((host/'stat').read_text().replace(') S ', ') Z '))
            with self.assertRaises(ValueError): observe_live_host({'pid':123,'pid_starttime':'456'}, root)

    def test_missing_thread_inventory_duplicate_children_and_cpu_reset_refuse(self):
        with Scratch('synthetic-soak-proc-inventory') as scratch:
            root = Path(scratch.path); host = self.proc_fixture(root)
            (host/'task'/'124'/'children').write_text('789 ')
            with self.assertRaisesRegex(ValueError, 'inconsistent'):
                observe_live_host({'pid':123,'pid_starttime':'456'}, root)
            (host/'task'/'124'/'children').write_text('790 ')
            raw = (host/'stat').read_bytes()
            from acceptance.suite import soak_resources
            read = soak_resources._read
            reads = iter([raw, raw.replace(b' 7 3 ', b' 6 3 ')])
            def resetting(path, maximum=65536):
                return next(reads) if path.name == 'stat' else read(path, maximum)
            with patch.object(soak_resources, '_read', side_effect=resetting):
                with self.assertRaisesRegex(ValueError, 'regressed'):
                    observe_live_host({'pid':123,'pid_starttime':'456'}, root)
            with patch.object(soak_resources, '_inventory', side_effect=[['0','1','2'], []]):
                with self.assertRaisesRegex(ValueError, 'thread inventory'):
                    observe_live_host({'pid':123,'pid_starttime':'456'}, root)

    @unittest.skipUnless(sys.platform == 'linux', 'actual Linux procfs observation')
    def test_actual_ordinary_owned_child_is_sampled_then_reaped(self):
        child = subprocess.Popen([sys.executable, '-c', 'import time; print("ready",flush=True); time.sleep(10)'],
                                 stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            self.assertEqual(child.stdout.readline(), b'ready\n')
            birth = Path('/proc',str(child.pid),'stat').read_bytes().rpartition(b') ')[2].split()[19].decode()
            sample = observe_live_host(dict(pid=child.pid,pid_starttime=birth))
            self.assertGreater(sample['metrics']['rss_bytes'], 0)
            self.assertEqual(sample['metrics']['file_descriptors'], 3)
            self.assertEqual(sample['metrics']['active_children'], 0)
            self.assertGreaterEqual(sample['metrics']['cpu_ns'], 0)
        finally:
            child.terminate(); child.wait(timeout=5)
            child.stdout.close()
        with self.assertRaises(FileNotFoundError): observe_live_host(dict(pid=child.pid,pid_starttime=birth))


if __name__ == '__main__':
    unittest.main()
