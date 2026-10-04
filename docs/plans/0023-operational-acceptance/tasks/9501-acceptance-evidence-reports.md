---
id: acceptance-evidence-reports
title: "Acceptance Evidence Reports"
workstream: "0095"
kind: task
depends_on: []
gated: false
touches:
  - acceptance/suite/run.py
  - acceptance/suite/evidence.py
  - acceptance/tests/test_evidence.py
  - acceptance/acceptance.sh
  - acceptance/Containerfile
status: planned
merged_as: ""
---
# Acceptance Evidence Reports

A terminal summary is useful during a run; a release needs evidence tied to
the binary and the checks that actually executed.

**Steps:**

1. Implement the `fsm.acceptance/1` report described in ARCHITECTURE, preserving
   the current human-readable output and recording the full selected/required
   scenario inventory, verdicts, assertion counts and artifact digests.
2. Add an explicit evidence output directory and candidate identity inputs;
   hash the tested binary, verify the supplied revision provenance, and mark
   dirty or unidentified builds ineligible for release evidence.
3. Preserve reports outside Podman's disposable container through an explicit
   output mount; do not mount host stores or credentials into the suite.
4. Make incomplete, zero-check, required-skip and filtered runs distinguishable
   from a full pass; preserve a failing exit even if report writing also fails.
5. Write synthetic reporter tests, including interrupted/partial report
   emission, using Python's standard library alone.

**Tests:**

- `python3 -m unittest discover -s acceptance/tests -p test_evidence.py` covers
  correct identity, wrong digest, dirty build, missing required scenario,
  zero checks, filtered inventory, skip, failure and incomplete report.
- A failing stub scenario exits nonzero and leaves a failed/incomplete report
  when the evidence directory becomes unwritable.
- A Podman run retains its complete report after container removal, and a
  host invocation with `FSM_BIN` records the actual executable's digest.

- **Done when:** the same scenario results produce matching verdicts in the
  console and validated evidence report, and every negative case above is
  rejected as release proof by the reporter tests and an installed-binary run.
