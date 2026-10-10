"""Labelled hardware/journal fault controls, never installed native proof."""
import copy
import json
import os
from pathlib import Path
import runpy
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from acceptance.suite.executor_crash import SYMBOL, validate_cut, claim_prefix
from acceptance.suite.fsm import Scratch


class ExactClaimCutTests(unittest.TestCase):
    def test_empty_debugger_argument_parameter_cannot_erase_the_original_stdio_command(self):
        commands = []
        def execute(command, **_kwargs):
            commands.append(command)
            if command == 'quit 1':
                raise SystemExit(1)
            if command == 'run':
                raise RuntimeError('labelled inferior would have launched')
            return ''
        breakpoint = SimpleNamespace(pending=True, type=2, locations=[SimpleNamespace(address=4096)])
        debugger = SimpleNamespace(execute=execute, parameter=lambda _name: '',
            BP_HARDWARE_BREAKPOINT=2, Breakpoint=lambda *_args, **_kwargs: breakpoint,
            selected_inferior=lambda: SimpleNamespace(pid=0))
        script = Path(__file__).resolve().parents[1] / 'fixtures/installed_debugger.py'
        with Scratch('labelled-empty-debugger-args') as scratch:
            binary = Path(scratch.write('binary', 'labelled executable stub'))
            for name in ('stdin', 'stdout', 'stderr'):
                os.mkfifo(Path(scratch.path) / (name + '.fifo'), 0o600)
            arguments = ['serve', '--execute', '--handlers=/labelled handlers', '--data-dir=/labelled store']
            symbols = [SimpleNamespace(stdout='00001000 T ' + SYMBOL + '\n'),
                SimpleNamespace(stdout='00001000 T _RNlabelled_symbol\n')]
            with patch.dict('sys.modules', {'gdb': debugger}), patch.dict('os.environ',
                dict(GITHUB_ACTIONS='true', FSM_ACCEPTANCE_DISPOSABLE_NATIVE='1',
                    FSM_BIN=str(binary), FSM_DEBUGGER_DIRECTORY=scratch.path,
                    FSM_DEBUGGER_TRANSPORT='stdio', FSM_DEBUGGER_ARGUMENTS=json.dumps(arguments))), \
                patch('subprocess.run', side_effect=symbols):
                with self.assertRaises(SystemExit):
                    runpy.run_path(str(script), run_name='__main__')
            launches = [command for command in commands if command.startswith('set args ')]
            self.assertEqual(len(launches), 1)
            self.assertTrue(launches[0].startswith("set args serve --execute '--handlers=/labelled handlers' '--data-dir=/labelled store' <"))
            self.assertNotIn('run', commands)

    def cut(self):
        return dict(schema='fsm.installed-hardware-cut/1', cut='claimed-before-binding',
            symbol=SYMBOL, breakpoint_type='hardware', breakpoint_hits=1,
            pc=4100, breakpoint_address=4100, original=dict(pid=123, pid_starttime='456'),
            raw_symbol='_RNlabelled_symbol', demangled_symbol=SYMBOL, symbol_offset=4096,
            all_threads_stopped=True, threads=2, binary_sha256='a'*64,
            mapped_code=[dict(begin=4096,end=8192,file_offset=4096,bytes=4096,
                mapped_sha256='b'*64,file_sha256='b'*64)])

    def test_software_or_approximate_stops_cannot_prove_the_exact_hardware_cut(self):
        original=self.cut()
        self.assertEqual(validate_cut(original,'a'*64),original)
        for change in (dict(breakpoint_type='software'),dict(symbol='another_function'),
            dict(breakpoint_hits=0),dict(breakpoint_hits=2),dict(breakpoint_hits=True),dict(pc=4101),
            dict(all_threads_stopped=False),dict(binary_sha256='c'*64),dict(mapped_code=[])):
            value={**original,**change}
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def test_modified_or_foreign_executable_code_cannot_prove_an_unchanged_candidate(self):
        for change in (dict(mapped_sha256='c'*64),dict(file_sha256='invalid'),
            dict(bytes=4097),dict(begin=4101,end=8197),dict(file_offset=-1)):
            value=copy.deepcopy(self.cut());value['mapped_code'][0].update(change)
            with self.subTest(change=change),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def test_original_process_identity_is_required(self):
        for original in ({},dict(pid=True,pid_starttime='456'),dict(pid=123,pid_starttime=''),
            dict(pid=123,pid_starttime='unknown')):
            value=self.cut();value['original']=original
            with self.subTest(original=original),self.assertRaises(ValueError):
                validate_cut(value,'a'*64)

    def prefix(self):
        return [dict(kind='event_applied',seq=1,body=dict(instance_id='fixture',event='start')),
            dict(kind='execution_claimed',seq=2,body=dict(instance_id='fixture',attempt=1,run_id=1))]

    def test_missing_duplicate_foreign_or_already_consumed_claims_refuse(self):
        records=self.prefix();self.assertEqual(claim_prefix(records,'fixture'),records[-1])
        values=[records[:-1],records+[records[-1]],records+[dict(kind='execution_stopped',seq=3,body={})]]
        for change in (dict(instance_id='foreign'),dict(attempt=2),dict(attempt=True)):
            value=copy.deepcopy(records);value[-1]['body'].update(change);values.append(value)
        for value in values:
            with self.subTest(value=value),self.assertRaises(ValueError):
                claim_prefix(value,'fixture')

    def test_pending_hardware_breakpoint_refuses_before_a_labelled_inferior_can_run(self):
        commands=[]
        def execute(command, **_kwargs):
            commands.append(command)
            if command=='quit 1': raise SystemExit(1)
            if command=='run': raise RuntimeError('labelled inferior would have launched')
        breakpoint=SimpleNamespace(pending=True,type=2,locations=[SimpleNamespace(address=4096)])
        debugger=SimpleNamespace(execute=execute,BP_HARDWARE_BREAKPOINT=2,
            Breakpoint=lambda *_args,**_kwargs:breakpoint,
            events=SimpleNamespace(stop=SimpleNamespace(connect=lambda _callback:None,disconnect=lambda _callback:None)),
            selected_inferior=lambda:SimpleNamespace(pid=0))
        symbols=[SimpleNamespace(stdout='00001000 T '+SYMBOL+'\n'),
            SimpleNamespace(stdout='00001000 T _RNlabelled_symbol\n')]
        script=Path(__file__).resolve().parents[1]/'fixtures/installed_debugger.py'
        with Scratch('labelled-pending-debugger-stub') as scratch:
            binary=Path(scratch.write('binary','labelled executable stub'))
            with patch.dict('sys.modules',{'gdb':debugger}), patch.dict('os.environ',
                dict(GITHUB_ACTIONS='true',FSM_ACCEPTANCE_DISPOSABLE_NATIVE='1',
                    FSM_BIN=str(binary),FSM_DEBUGGER_DIRECTORY=scratch.path)),\
                patch('os.geteuid',return_value=1000),patch('subprocess.run',side_effect=symbols):
                with self.assertRaises(SystemExit): runpy.run_path(str(script),run_name='__main__')
        self.assertNotIn('run',commands)
