"""Disposable GDB observer: stop unchanged installed executables at exact cuts.

Loaded by GDB's Python interpreter, never by the installed executable; the
hardware breakpoint changes no candidate instruction or fixture outcome.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import stat
import subprocess
import time

import gdb

CUT = os.environ.get('FSM_DEBUGGER_CUT', 'claimed-before-binding')
AUTHORITY_SYMBOLS = {
    'spawn-before-submission': 'fsm_containment_authority::authority::launch::begin',
    'authorization-before-grant': 'fsm_containment_authority::authority::authorize::publish_enrolled',
    'candidate-before-fence': 'fsm_containment_authority::authority::runner::observed_candidate',
    'closed-before-result-publication': 'fsm_containment_authority::authority::runner::completion_record::publish'}
METHODS = {'claimed-before-binding': 'start_native',
    'stopped-before-settlement': 'settle_native_stopped',
    'acked-before-event': 'deliver_native_handoff',
    'event-after-advance': 'deliver_native_handoff'}
ROOT_CUT = CUT in AUTHORITY_SYMBOLS
SYMBOL = AUTHORITY_SYMBOLS[CUT] if ROOT_CUT else 'fsm_execute::run::pipeline::Pipeline::' + METHODS[CUT]
DEMANGLED = (SYMBOL,) if ROOT_CUT else (SYMBOL, '<fsm_execute::run::pipeline::Pipeline>::' + METHODS[CUT])


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
        if ROOT_CUT:
            os.fchmod(stream.fileno(), 0o444)
        os.fsync(stream.fileno())
    pending.rename(directory / (name + '.json'))
    descriptor = os.open(directory, os.O_RDONLY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)


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


def stopped_observation(inferior, breakpoint, stops, expected, binary, binary_hash, symbol, cut, diagnostic):
    locations = breakpoint.locations
    threads = inferior.threads()
    if (stops != expected or breakpoint.type != gdb.BP_HARDWARE_BREAKPOINT
        or breakpoint.hit_count != 1 or len(locations) != 1
        or not threads or len(threads) > 256 or not all(thread.is_stopped() for thread in threads)
        or gdb.selected_frame().pc() != locations[0].address):
        raise ValueError('the exact original hardware breakpoint did not stop every thread')
    return dict(schema='fsm.installed-hardware-cut/1', cut=cut,
        symbol=SYMBOL, breakpoint_type='hardware', breakpoint_hits=1,
        pc=gdb.selected_frame().pc(), breakpoint_address=locations[0].address,
        original=identity(inferior.pid), all_threads_stopped=True, threads=len(threads),
        binary_sha256=binary_hash, mapped_code=code_observations(inferior, binary),
        debugger_version=gdb.VERSION, startup_diagnostic=diagnostic, **symbol)


def main():
    if (os.environ.get('GITHUB_ACTIONS') != 'true'
        or os.environ.get('FSM_ACCEPTANCE_DISPOSABLE_NATIVE') != '1'
        or (os.geteuid() == 0) is not ROOT_CUT):
        raise ValueError('installed debugger requires its exact disposable CI execution role')
    directory = Path(os.environ['FSM_DEBUGGER_DIRECTORY']).resolve()
    binary = Path(os.environ['FSM_BIN']).resolve()
    if ROOT_CUT:
        metadata = directory.lstat()
        executable = binary.lstat()
        if (binary != Path('/usr/libexec/fsm-containment-authority')
            or not stat.S_ISREG(executable.st_mode) or executable.st_uid != 0
            or executable.st_mode & 0o022 or metadata.st_uid != 0
            or not stat.S_ISDIR(metadata.st_mode) or metadata.st_mode & 0o022
            or not re.fullmatch('/usr/libexec/fsm-acceptance-[a-f0-9]{32}/debugger', str(directory))):
            raise ValueError('Root observer requires the protected installed helper and fresh fixture directory')
    original_hash = hashlib.sha256(binary.read_bytes()).hexdigest()
    for command in ('set pagination off', 'set confirm off', 'set non-stop off',
                    'set language c', 'set breakpoint pending off',
                    'set follow-fork-mode parent', 'set detach-on-fork on', 'target native'):
        gdb.execute(command, to_string=True)
    transport = os.environ.get('FSM_DEBUGGER_TRANSPORT', 'standalone')
    if ROOT_CUT and transport != 'standalone':
        raise ValueError('Root helper observation cannot redirect candidate stdio')
    if transport == 'stdio':
        redirects = []
        for number, name in enumerate(('stdin', 'stdout', 'stderr')):
            path = directory / (name + '.fifo')
            metadata = path.lstat()
            if not stat.S_ISFIFO(metadata.st_mode) or metadata.st_uid != os.geteuid() or metadata.st_mode & 0o077:
                raise ValueError('stdio cut requires separate private task-owned FIFOs')
            redirects.append(('<' if number == 0 else '>' if number == 1 else '2>') + shlex.quote(str(path)))
        arguments = gdb.parameter('args')
        if not isinstance(arguments, str) or len(arguments.encode()) > 16_384:
            raise ValueError('original stdio launch arguments exceed their bound')
        gdb.execute('set environment SHELL /bin/sh', to_string=True)
        gdb.execute('set startup-with-shell on', to_string=True)
        gdb.execute('set args ' + arguments + ' ' + ' '.join(redirects), to_string=True)
    elif transport != 'standalone':
        raise ValueError('unknown installed hardware-observer transport')
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
        if transport == 'stdio':
            publish(directory, 'stdio-launch', dict(binary_sha256=original_hash, transport=transport))
        startup = gdb.execute('run', to_string=True)
        inferior = gdb.selected_inferior()
        first_cut = 'acked-before-event' if CUT == 'event-after-advance' else CUT
        ready = stopped_observation(inferior, breakpoint, stops, [True], binary,
            original_hash, symbol, first_cut, startup)
        if CUT == 'event-after-advance':
            entry = ready
            frame = gdb.selected_frame()
            architecture = frame.architecture().name()
            if architecture != 'i386:x86-64' or frame.older() is None:
                raise ValueError('exact return observation requires the supported original x86-64 call frame')
            stack = int(frame.read_register('rsp'))
            address = int.from_bytes(bytes(inferior.read_memory(stack, 8)), 'little')
            caller_pc = frame.older().pc()
            thread = gdb.selected_thread().global_num
            if address <= 0 or address != caller_pc or not any(row['begin'] <= address < row['end']
                    for row in entry['mapped_code']):
                raise ValueError('original stack return address is not the original executable caller')
            breakpoint.enabled = False
            breakpoint = gdb.Breakpoint('*' + hex(address), type=gdb.BP_HARDWARE_BREAKPOINT)
            if breakpoint.pending or breakpoint.type != gdb.BP_HARDWARE_BREAKPOINT or len(breakpoint.locations) != 1:
                raise ValueError('the original return address requires an exact hardware breakpoint')
            continuation = gdb.execute('continue', to_string=True)
            ready = stopped_observation(inferior, breakpoint, stops, [True, True], binary,
                original_hash, symbol, CUT, continuation)
            returned_stack = int(gdb.selected_frame().read_register('rsp'))
            returned_thread = gdb.selected_thread().global_num
            if (ready['original'] != entry['original'] or ready['mapped_code'] != entry['mapped_code']
                or returned_stack != stack + 8 or returned_thread != thread):
                raise ValueError('the original unchanged call frame did not return on its original thread')
            ready.update(position='return', entry=entry, return_provenance=dict(architecture=architecture,
                entry_stack_pointer=stack, return_stack_pointer=returned_stack, stack_return_address=address,
                caller_pc=caller_pc, entry_thread=thread, return_thread=returned_thread))
        original = ready['original']
        publish(directory, 'ready', ready)
        deadline = time.monotonic() + 20
        while not (directory / 'kill').exists():
            if time.monotonic() >= deadline:
                raise ValueError('original hardware cut expired without explicit owned termination')
            time.sleep(0.01)
        if identity(inferior.pid) != original:
            raise ValueError('original debugged process identity changed before termination')
        # Linux GDB's owned-inferior kill avoids resuming a selected thread
        # while its siblings disappear; the original process must be reaped.
        diagnostic = gdb.execute('kill', to_string=True)
        if 'killed' not in diagnostic.lower() or inferior.pid != 0:
            raise ValueError('original inferior forced retirement is not confirmed')
        if hashlib.sha256(binary.read_bytes()).hexdigest() != original_hash:
            raise ValueError('original installed executable bytes changed')
        publish(directory, 'retired', dict(original=original, mechanism='gdb-owned-inferior-kill',
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
