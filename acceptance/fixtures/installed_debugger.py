"""Disposable GDB observer: stop an unchanged installed CLI before binding.

Loaded by GDB's Python interpreter, never by the installed executable; the
hardware breakpoint changes no candidate instruction or fixture outcome.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time

import gdb

SYMBOL = 'fsm_execute::run::pipeline::Pipeline::start_native'
DEMANGLED = (SYMBOL, '<fsm_execute::run::pipeline::Pipeline>::start_native')


def resolve_symbol(binary):
    outputs = []
    for arguments in (['-C'], []):
        result = subprocess.run(['nm', '--defined-only', *arguments, str(binary)],
            capture_output=True, text=True, check=True, timeout=10)
        if len(result.stdout.encode()) > 16_777_216:
            raise ValueError('original installed symbol inventory exceeds its bound')
        outputs.append(result.stdout.splitlines())
    matches = [row.split(maxsplit=2) for row in outputs[0]
        if len(row.split(maxsplit=2)) == 3 and row.split(maxsplit=2)[2] in DEMANGLED]
    if len(matches) != 1 or matches[0][1] not in ('t', 'T'):
        raise ValueError('the exact installed pre-binding symbol is missing or ambiguous')
    offset, kind, demangled = matches[0]
    raw = [row.split(maxsplit=2)[2] for row in outputs[1]
        if len(row.split(maxsplit=2)) == 3 and row.split(maxsplit=2)[:2] == [offset, kind]]
    if len(raw) != 1 or not re.fullmatch(r'_(?:R|ZN)[A-Za-z0-9_]+', raw[0]):
        raise ValueError('the exact original Rust symbol is not unique')
    return dict(raw_symbol=raw[0], demangled_symbol=demangled, symbol_offset=int(offset, 16))


def publish(directory, name, value):
    pending = directory / (name + '.pending')
    with pending.open('x', encoding='utf-8') as stream:
        json.dump(value, stream, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    pending.rename(directory / (name + '.json'))


def identity(pid):
    encoded = Path('/proc', str(pid), 'stat').read_bytes()
    fields = encoded.rpartition(b') ')[2].split()
    if len(encoded) > 4096 or len(fields) < 20 or not fields[19].isdigit():
        raise ValueError('debugged original process identity is invalid')
    return dict(pid=pid, pid_starttime=fields[19].decode('ascii'))


def code_observations(inferior, binary):
    encoded = Path('/proc', str(inferior.pid), 'maps').read_text()
    if len(encoded.encode()) > 65_536:
        raise ValueError('original executable map exceeds its bound')
    observations = []
    for row in encoded.splitlines():
        fields = row.split(maxsplit=5)
        if len(fields) != 6 or fields[1] != 'r-xp' or fields[5] != str(binary):
            continue
        begin, end = (int(value, 16) for value in fields[0].split('-'))
        offset = int(fields[2], 16)
        size = end - begin
        if not 0 < size <= 16_777_216 or len(observations) >= 4:
            raise ValueError('original executable code exceeds its bound')
        with binary.open('rb') as stream:
            stream.seek(offset)
            expected = stream.read(size)
        if len(expected) != size:
            raise ValueError('original executable code map exceeds its file')
        observed = bytes(inferior.read_memory(begin, size))
        if observed != expected:
            raise ValueError('debugger changed original executable instructions')
        observations.append(dict(begin=begin, end=end, file_offset=offset, bytes=size,
            mapped_sha256=hashlib.sha256(observed).hexdigest(),
            file_sha256=hashlib.sha256(expected).hexdigest()))
    if not observations:
        raise ValueError('the original executable code map is missing')
    return observations


def main():
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
        or os.environ.get('FSM_ACCEPTANCE_DISPOSABLE_NATIVE') != '1' or os.geteuid() == 0):
        raise ValueError('installed debugger requires the ordinary disposable CI operator')
    directory = Path(os.environ['FSM_DEBUGGER_DIRECTORY']).resolve()
    binary = Path(os.environ['FSM_BIN']).resolve()
    original_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    for command in ('set pagination off', 'set confirm off', 'set non-stop off',
                    'set language c', 'set breakpoint pending off',
                    'set follow-fork-mode parent', 'set detach-on-fork on'):
        gdb.execute(command, to_string=True)
    symbol = resolve_symbol(binary)
    breakpoint = gdb.Breakpoint("*'" + symbol['raw_symbol'] + "'", type=gdb.BP_HARDWARE_BREAKPOINT)
    if breakpoint.pending or breakpoint.type != gdb.BP_HARDWARE_BREAKPOINT or len(breakpoint.locations) != 1:
        raise ValueError('a missing or pending hardware cut cannot launch the original executable')
    stops = []

    def stop(event):
        stops.append(isinstance(event, gdb.BreakpointEvent)
            and len(event.breakpoints) == 1 and event.breakpoints[0] == breakpoint)

    gdb.events.stop.connect(stop)
    try:
        startup = gdb.execute('run', to_string=True)
        inferior = gdb.selected_inferior()
        locations = breakpoint.locations
        threads = inferior.threads()
        if (stops != [True] or breakpoint.type != gdb.BP_HARDWARE_BREAKPOINT
            or breakpoint.hit_count != 1 or len(locations) != 1
            or not threads or len(threads) > 256 or not all(thread.is_stopped() for thread in threads)
            or gdb.selected_frame().pc() != locations[0].address):
            raise ValueError('the exact original hardware breakpoint did not stop every thread')
        original = identity(inferior.pid)
        mapped = code_observations(inferior, binary)
        ready = dict(schema='fsm.installed-hardware-cut/1', cut='claimed-before-binding',
            symbol=SYMBOL, breakpoint_type='hardware', breakpoint_hits=1,
            pc=gdb.selected_frame().pc(), breakpoint_address=locations[0].address,
            original=original, all_threads_stopped=True, threads=len(threads),
            binary_sha256=original_hash, mapped_code=mapped, debugger_version=gdb.VERSION,
            startup_diagnostic=startup, **symbol)
        publish(directory, 'ready', ready)
        deadline = time.monotonic() + 20
        while not (directory / 'kill').exists():
            if time.monotonic() >= deadline:
                raise ValueError('original hardware cut expired without explicit owned termination')
            time.sleep(0.01)
        if identity(inferior.pid) != original:
            raise ValueError('original debugged process identity changed before termination')
        diagnostic = gdb.execute('signal SIGKILL', to_string=True)
        if 'SIGKILL' not in diagnostic or inferior.pid != 0:
            raise ValueError('original inferior SIGKILL retirement is not confirmed')
        if hashlib.sha256(binary.read_bytes()).hexdigest() != original_hash:
            raise ValueError('original installed executable bytes changed')
        publish(directory, 'retired', dict(original=original, signal='SIGKILL',
            diagnostic=diagnostic, inferior_pid=inferior.pid, binary_sha256=original_hash))
    finally:
        gdb.events.stop.disconnect(stop)
        # GDB owns this inferior; the service also has a finite runtime and
        # control-group termination, so observer failure cannot strand it.
        if gdb.selected_inferior().pid:
            gdb.execute('kill', to_string=True)


try:
    main()
except Exception as error:
    print('installed hardware observer failed: ' + str(error), flush=True)
    gdb.execute('quit 1')
