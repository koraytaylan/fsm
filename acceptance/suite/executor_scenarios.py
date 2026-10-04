"""Independent external-effect observations for installed executor scenarios.

This module does not import an engine, poll a workflow, or repair its state.
Expected mutation order comes from the handwritten scenario, not candidate
output. The transport scenarios and native shutdown matrix remain to be wired
once their production prerequisites exist.
"""
from __future__ import annotations

from dataclasses import dataclass
from collections.abc import Iterable, Mapping

MAX_TRACE_EVENTS = 100_000
MAX_TRACE_TEXT = 256


def _name(value) -> bool:
    return isinstance(value, str) and 0 < len(value) <= MAX_TRACE_TEXT


@dataclass(frozen=True)
class Observation:
    violations: tuple[str, ...]
    mutations: tuple[tuple[str, str], ...]
    peak_concurrency: dict[str, int]

    @property
    def passed(self) -> bool:
        return not self.violations

    def assert_passed(self) -> None:
        if self.violations:
            raise AssertionError("; ".join(self.violations))


def observe_trace(events: Iterable[dict], expected: Mapping[str, list[str]], *,
                  forbidden: frozenset[str] = frozenset(), complete: bool = False,
                  max_events: int = MAX_TRACE_EVENTS) -> Observation:
    """Check a complete fixture trace against a separately supplied ledger.

    Events have a strictly increasing integer seq and kind start/mutation/end.
    Every event names run and resource; mutations additionally name operation.
    The caller must establish trace completion independently; reaching EOF on
    a file that a live fixture may still append is insufficient. This observes
    fixture concurrency, not native process-tree containment or journal acks.
    """
    if not isinstance(max_events, int) or isinstance(max_events, bool) or not 0 < max_events <= MAX_TRACE_EVENTS:
        raise ValueError("trace limit must be within the hard ceiling")
    if not isinstance(complete, bool):
        raise ValueError("completion must be an independently established boolean")
    if (not isinstance(expected, Mapping) or not isinstance(forbidden, frozenset)
        or any(not _name(resource) for resource in forbidden)):
        raise ValueError("ledger and refusal obligations must name resources")
    if not expected and not forbidden:
        raise ValueError("observation must have an expected ledger or a refusal obligation")
    if set(expected) & forbidden:
        raise ValueError("an expected resource cannot also be forbidden")
    if any(not _name(resource) or not isinstance(operations, list)
           or any(not _name(operation) for operation in operations)
           for resource, operations in expected.items()):
        raise ValueError("expected ledger must name resources and operation lists")
    violations = []
    active: dict[str, str] = {}
    seen = set()
    started_resources = set()
    counts: dict[str, int] = {}
    peak: dict[str, int] = {}
    mutations = []
    last_seq = -1
    for index, event in enumerate(events):
        if index >= max_events:
            violations.append("trace/limit")
            break
        if not isinstance(event, dict):
            violations.append("trace/shape")
            break
        kind = event.get("kind")
        fields = {"seq", "kind", "run", "resource"} | ({"operation"} if kind == "mutation" else set())
        seq, run, resource = event.get("seq"), event.get("run"), event.get("resource")
        if (set(event) != fields or not isinstance(kind, str) or kind not in {"start", "mutation", "end"}
            or not isinstance(seq, int) or isinstance(seq, bool) or seq < 0
            or not _name(run) or not _name(resource)
            or (kind == "mutation" and not _name(event["operation"]))):
            violations.append("trace/shape")
            break
        if seq <= last_seq:
            violations.append("trace/order")
            break
        last_seq = seq
        if resource in forbidden:
            violations.append("executor/forbidden_side_effect")
        elif resource not in expected:
            violations.append("executor/unexpected_resource")
        if kind == "start":
            if run in seen:
                violations.append("executor/duplicate_run")
                break
            seen.add(run)
            started_resources.add(resource)
            active[run] = resource
            counts[resource] = counts.get(resource, 0) + 1
            peak[resource] = max(peak.get(resource, 0), counts[resource])
            if counts[resource] > 1:
                violations.append("executor/overlap")
        elif active.get(run) != resource:
            violations.append("executor/unowned_observation")
            break
        elif kind == "mutation":
            mutations.append((resource, event["operation"]))
        else:
            del active[run]
            counts[resource] -= 1
    if active:
        violations.append("executor/unfinished_run")
    for resource, operations in expected.items():
        observed = [operation for name, operation in mutations if name == resource]
        if resource not in started_resources:
            violations.append("executor/missing_progress")
        if observed != operations:
            violations.append("executor/missing_progress" if len(observed) < len(operations) else "executor/mutation_order")
    if not complete:
        violations.append("trace/incomplete")
    return Observation(tuple(dict.fromkeys(violations)), tuple(mutations), peak)
