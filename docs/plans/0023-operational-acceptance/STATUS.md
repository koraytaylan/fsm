# Plan 0023 — Operational Acceptance — Unregistered

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file;
task-level truth lives in [tasks/](tasks/) frontmatter, with lifecycle changes
owned by the integration coordinator.

- **Status:** Unregistered; independent evidence reporter landed in `0e00d30`; integrated operational acceptance is not complete.
- **Goal:** substantiate all five review concerns with installed-binary,
  sustained native and uncoached live-model evidence on an identified candidate.
- **Root cause:** existing automated tests and manually required host checks
  do not themselves establish sustained operation or retain a complete
  candidate-specific acceptance record.
- **Approach:** extend the independent Python acceptance suite, capture
  fail-closed structured reports, exercise the integrated executor contracts
  over both transports, and require native duration and real-model evidence
  before release closure.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped; one task explicitly gated
  on a real model, supported host access and human review.
- **Integration:** `unregistered`; run —; planning baseline `develop` @
  `67ad5e3`; validation base —; mode —; final integration —; register after
  plans 0020, 0021 and 0022 integrate, including 0022's platform decision.
- **Exceptions:** none granted; missing credentials, native evidence or a
  resolved lifecycle prerequisite cannot be counted as a pass.
- **Reporter progress:** `acceptance/suite/evidence.py` now retains atomic
  `fsm.acceptance/1` reports, incomplete markers, scenario inventories,
  assertions, candidate observations and diagnostic digests. The runner returns
  nonzero for required skips and zero-check scenarios, and the Podman wrapper
  mounts a separate evidence output directory. Thirteen synthetic reporter tests
  pass. A locally installed stable binary passed the filtered golden-loop
  scenario with 11 assertions; independent digest checks matched the binary
  and retained diagnostic log. This filtered, dirty-source run is explicitly
  ineligible for release. The consumer-install Podman golden-loop run also
  passed with 11 assertions and retained its report and matching diagnostic
  digest after container removal. A full consumer-install Podman run passed
  all 15 existing scenarios and 100 assertions, retaining the controlled-build
  receipt and diagnostics with verified digests. The source snapshot is dirty,
  so the report remains ineligible for release despite verified provenance.
  A frozen temporary verification repository at
  `5f176ce2cfc164dd798c01dc95dbdf5dc5b51b39` also passed all 15 scenarios and
  100 assertions through consumer installation, with clean source, matching
  controlled-build provenance and a validated retained report. This is a
  verification candidate, not a landing OID or integrated release candidate.
  Reporter implementation landed at
  `0e00d30313cbd419ceee36757b65d9073562a277`. Frozen review of
  `1b3451a..0e00d30` and a clean isolated checkout passed all 13 reporter
  tests and all 15 consumer-install scenarios / 100 assertions. The retained
  report and both artifact digests validate against that exact commit.
  This completes the reporter implementation evidence, while this plan's
  registration and task lifecycle remain pending the 0020–0022 integration
  prerequisites; no integrated operational-acceptance completion is claimed.
- **Reporter self-review:** verified that matching operator-supplied bytes
  cannot attest build origin, added an independent dirty-state rejection,
  and checked report-write failure preserves a nonzero exit and incomplete
  evidence. Further self-review added checks for ignored/assume-unchanged
  build inputs, source mutation during compilation, unknown verdicts,
  nonboolean assertions and tampered artifacts. Corrected provenance's
  version invocation to the actual `fsm version` command. Receipts come from
  the controlled build recipe, not a signed third-party attestation; release
  closure must establish trust in their producer. Frozen reporter review is
  complete on the range above. Stable workspace debug/release runs completed; their
  five failed environment-sensitive targets passed on rerun with local
  sockets enabled and isolated Git settings. Workspace clippy, formatting,
  warning-free documentation and file-size checks passed. Native macOS and
  Windows evidence has not been produced.
- **Installed-acceptance preparation:** the portability portion of task
  9502 landed independently in `7c8c49cd854c37b0772924515913835488c9df41`.
  Existing success/failure handlers now use `sys.executable` and an independent
  Python fixture; decimal generation uses the same explicit interpreter.
  The controlled source inventory includes `acceptance/fixtures`, with a
  separate inventory/digest test. All 16 Python self-tests pass (13 reporter
  and three fixture tests). The full consumer-installed Linux suite passed
  15 scenarios / 100 assertions using these fixtures, at Rust 1.89.0.
  Retained evidence is
  `/tmp/fsm-portable-fixtures-evidence/eaa9de43bb654600a1a35268ea47e80a/report.json`;
  its receipt includes the fixture source and its bundle digests validate.
  This is a dirty snapshot of `1f97188`, explicitly release-ineligible.
  Review of the portable change preserves the original assertions and
  confirms no engine imports in the fixture. The installed executor matrix,
  observer fault tests and native macOS/Windows execution remain incomplete.
- **Observer preparation:** task 9502 now has a pure independent trace
  observer landed independently in
  `9903ef39291ed26a9e0b220d304d1223f86e3dcd`. Thirteen fault/ledger tests cover missing work,
  mutation ordering, side effects after refusal, overlapping attempts, stuck
  runs, duplicate/unknown ownership, malformed input and inclusive limits.
  Review added typed-kind rejection and requires an observed invocation even
  for an expected zero-mutation operation. It requires independently proved
  trace completion and never polls or repairs a candidate. All 29 Python
  self-tests pass (13 reporter, three fixture, thirteen observer). Review also
  refuses malformed/vacuous ledgers and accepts concurrency only across
  distinct resources. Trace text has a 256-character hard ceiling, with
  exact-limit and limit-plus-one tests. Formatting, Clippy, docs, zero-dependency and embedding
  checks pass; full stable workspace debug/release gates pass on Linux.
  Frozen review of `080be626e05f913f67ec59963f7631d25fe5a1ac..9903ef39291ed26a9e0b220d304d1223f86e3dcd`
  is complete with the malformed-kind, vacuous-ledger and text-limit repairs. Real fixture trace
  production, installed transport wiring and the native lifecycle matrix are
  still pending; synthetic observer results are not candidate acceptance.
- **Outcome:** pending; readiness claims will name the evidence that supports
  them, and a missing proof will prevent closure.

_Task frontmatter is authoritative; this file is the roll-up._
