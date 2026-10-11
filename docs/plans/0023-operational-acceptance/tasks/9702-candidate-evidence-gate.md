---
id: candidate-evidence-gate
title: "Candidate Evidence Gate"
workstream: "0097"
kind: task
depends_on:
  - acceptance-evidence-reports
  - native-operational-evidence
  - live-model-acceptance-protocol
gated: false
touches:
  - acceptance/verify-candidate.py
  - acceptance/tests/test_candidate.py
  - acceptance/candidate-schema.json
  - acceptance/suite/evidence.py
  - .github/workflows/release.yml
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Candidate Evidence Gate

A release decision must reject a green report from the wrong build or a
partial matrix as readily as a red report from the right one.

**Steps:**

1. Define an evidence index binding candidate code SHA, binary digests,
   implementation contracts, required automated/native/manual reports and
   existing release gates to stable artifact references and content digests.
2. Implement a standard-library validator which verifies inventory, identity,
   verdicts, provenance and manual reviewer records, distinguishing an
   evidence-only documentation commit from the tested code revision.
3. Reject dirty/stale candidates, untrusted or missing artifacts, required
   skips, zero-check results, unresolved findings and incomplete sustained
   runs; report exact missing proof rather than a generic failure.
4. Integrate a pre-tag invocation into RELEASE and recheck immutable evidence
   before tag publication in the release workflow; retain existing gate,
   fuzz, dependency, library-consumer and version checks.

**Tests:**

- `python3 -m unittest discover -s acceptance/tests -p test_candidate.py`
  accepts a complete synthetic bundle and rejects one-at-a-time removal or
  corruption of every required field, axis, scenario, digest and review record.
- Replacing a passed candidate's binary or report makes verification fail.
- A complete real candidate bundle passes, and release publication cannot run
  when evidence validation fails or an evidence artifact cannot be fetched.

- **Done when:** the validator's adversarial bundle tests and a real complete
  candidate run pass, and a missing or stale mandatory proof prevents the
  release publication path from executing.
