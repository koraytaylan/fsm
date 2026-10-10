"""Seeded operational schedule and completion rules, independent of the engine.

Duration, completed work, workload coverage and continued progress are separate
obligations; reaching a count and then sleeping cannot establish a soak pass.
"""
from collections import Counter
import time

CASES = ('success', 'retry', 'deadline', 'cancel', 'compensate', 'manual',
         'churn', 'noise', 'contention', 'archive', 'reopen', 'replacement')
FLOORS = {'smoke': (120, 100), 'sustained': (28_800, 10_000)}
NANOSECONDS = 1_000_000_000


def bounded_integer(value, minimum, maximum, label):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError(label + ' must be a bounded integer')
    return value


def validate_profile(profile):
    if not isinstance(profile, dict) or profile.get('name') not in FLOORS:
        raise ValueError('unknown operational profile')
    duration, count = FLOORS[profile['name']]
    bounded_integer(profile.get('minimum_seconds'), duration, 86_400, 'duration floor')
    bounded_integer(profile.get('minimum_cycles'), count, 1_000_000, 'cycle floor')
    bounded_integer(profile.get('maximum_seconds'), profile['minimum_seconds'] + 1,
                    172_800, 'maximum duration')
    bounded_integer(profile.get('watchdog_seconds'), 1,
                    profile['maximum_seconds'] - 1, 'no-progress watchdog')
    bounded_integer(profile.get('maximum_cycles'), profile['minimum_cycles'],
                    10_000_000, 'maximum cycles')
    return profile


def schedule(seed, maximum_cycles):
    """Every twelve-cycle block mixes all faults in a seeded finite order."""
    state = bounded_integer(seed, 1, (1 << 64) - 1, 'seed')
    maximum_cycles = bounded_integer(maximum_cycles, 1, 10_000_000, 'schedule size')
    index = 0
    while index < maximum_cycles:
        block = list(CASES)
        for cursor in range(len(block) - 1, 0, -1):
            state ^= state >> 12
            state ^= (state << 25) & ((1 << 64) - 1)
            state ^= state >> 27
            selected = ((state * 2_685_821_657_736_338_717) & ((1 << 64) - 1)) % (cursor + 1)
            block[cursor], block[selected] = block[selected], block[cursor]
        for case in block:
            if index == maximum_cycles:
                return
            # Keep one twelve-case block in the same host; six blocks cover
            # every case on both handler kinds and all three host frontends.
            block_number = index // len(CASES)
            yield dict(index=index, case=case,
                       handler_kind=('process', 'mcp')[block_number % 2],
                       transport=('standalone', 'stdio', 'http')[(block_number // 2) % 3])
            index += 1


class RunState:
    """A caller records only independently verified, fully completed cycles."""

    def __init__(self, profile, clock=time.monotonic_ns):
        self.profile = validate_profile(dict(profile))
        self.clock = clock
        self.started = bounded_integer(clock(), 0, (1 << 63) - 1, 'clock')
        self.previous = self.started
        self.last_progress = self.started
        self.completed = 0
        self.coverage = Counter()
        self.reason = None

    def now(self):
        observed = bounded_integer(self.clock(), 0, (1 << 63) - 1, 'clock')
        if observed < self.previous:
            self.reason = 'clock_regressed'
        self.previous = observed
        return observed

    def verdict(self):
        now = self.now()
        elapsed = now - self.started
        if self.reason is not None:
            return 'incomplete'
        if elapsed >= self.profile['maximum_seconds'] * NANOSECONDS:
            self.reason = 'maximum_duration'
            return 'incomplete'
        if now - self.last_progress >= self.profile['watchdog_seconds'] * NANOSECONDS:
            self.reason = 'no_progress'
            return 'incomplete'
        if (self.last_progress - self.started >= self.profile['minimum_seconds'] * NANOSECONDS
            and self.completed >= self.profile['minimum_cycles']
            and set(self.coverage) == set(CASES)):
            return 'passed'
        if self.completed >= self.profile['maximum_cycles']:
            self.reason = 'maximum_cycles_before_duration'
            return 'incomplete'
        return 'running'

    def complete(self, entry, completed_ns=None):
        if self.verdict() != 'running':
            raise ValueError('a terminal operational run cannot accept another cycle')
        if (not isinstance(entry, dict) or type(entry.get('index')) is not int
            or entry['index'] != self.completed or entry.get('case') not in CASES):
            self.reason = 'missing_or_duplicate_cycle'
            raise ValueError('completed operational cycles must follow their exact schedule')
        completion=self.previous if completed_ns is None else completed_ns
        if type(completion) is not int or not self.last_progress<=completion<=self.previous:
            self.reason='invalid_completion_clock'
            raise ValueError('actual completion must use a non-regressed observed clock')
        self.completed += 1
        self.coverage[entry['case']] += 1
        self.last_progress = completion
