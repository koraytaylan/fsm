# Shutdown ownership review

Task 9402 requires immediate admission closure for the addressed executor
incarnation, including work published before binding; observing another live
executor's claim does not authorize closing its native domain.

## Current source findings

`run/native_owners.rs::adopt` retains all observation execution owners through
`retain` before refreshing admissions; `retain` has no local admission provenance
field. `install` marks request/entry state before fallible scheduler and helper
operations, but these flags encode transport behavior, not ownership origin.
`apply` retains a successfully published claim even when installation fails.
These paths must preserve local ownership without classifying every recovered
claim as locally admitted.

`run/native_admission.rs::take_ready` changes the prepared phase to
ClaimUncertain before the fallible claim append. `refresh` subsequently matches
the genuine observed claim to the original prepared domain, retains scheduler
capacity and removes the pending admission. Because `NativeOwners::adopt`
already retained this observation, local publication provenance must transfer
into that owner before the admission entry disappears; otherwise a later stop
cannot distinguish it from a foreign observation. Matching only effect identity
is insufficient: the original domain and complete claim identity are required.

## Required implementation and evidence

Keep local admission provenance independently of helper phase and retain it
through uncertain publication, failed binding and completion. Observed/recovered
claims remain protected against replacement and may recover authenticated
completion, but observation alone must not request active closure. Control must
bind to the exact local endpoint incarnation, not infer liveness from journal
phase, missing launch files or PID absence.

Cover queued, preparing, prepared, publication-uncertain, published-before-bind,
bound and executing local phases. Actual paired-executor tests must stop one
incarnation without closing the other's domain; an uncertain local append must
retain stop responsibility without admitting replacement work. Receipt-only
interruption supplies settlement after authenticated closure, but does not
solve pre-binding proof or exclude another live owner.

This review changes no runtime behavior or acceptance status; task 9402 remains
planned pending the production control/pump and its native runtime evidence.
