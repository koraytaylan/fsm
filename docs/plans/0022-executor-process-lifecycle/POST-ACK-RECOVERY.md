# Post-ack recovery implementation review

Status: atomic store/fold persistence implemented with targeted MSRV proof;
full host/native acceptance and cold host delivery remain unfinished.
This review preserves task 9401's full production and crash-recovery scope.

## Observed gap

`ExecutionState::settle(Acked)` removes the original claim and retry ledger.
`apply_settled` now validates optional original handoff material against the
verified claim anchor, stopped result and actual acknowledgement identity,
and installs it in that same transaction. Native completion settlement
forwards its checked original event contract; acknowledgements without
a declared event, attempts and interruptions create no handoff. `NativeOwners::Owner` retains the checked original
handler and completion only in memory; its disabled-event parking survives
ordinary ticks, not executor death. A new host adopts unresolved claims only.

`Store::replay_execution_settlement` requires an actual replay response with
the original outcome/result, rather than accepting an idempotency fingerprint
as an acknowledgement result. This is correct and must remain strict: sealed
dedup metadata can prove that a key was claimed without preserving its actual
result, and a claimed event key can name rejection rather than acceptance.
The base execution-claim hash index intentionally covers unresolved claims
only, so it cannot reconstruct a consumed original claim after sealing.

## Required persistent transition

The native acknowledgement transaction must retain a bounded handoff when
the original checked contract declares an outcome event. Its closed material
must include the complete original Claim, original claim-record hash, checked
handler contract, actual terminal stopped outcome, acknowledgement request key
and sequence, and the exact derived event request key. The contract fingerprint
must match the original claim using `fsm:handler-contract:1`; the event choice
must follow the actual acknowledged outcome, including terminal exhaustion,
and must never consult the current handler table.

The handoff must be installed by the same replay transition that consumes the
stopped claim and acknowledges the effect: a later append would leave another
crash window. The store writer must validate the original claim hash and
contract before append, and the pure fold must validate the handoff against
the original claim, stopped result, request identity and acknowledgement
sequence before installing it. Missing, contradictory, oversized or foreign
material must refuse publication/replay rather than creating a guessed event.
The original hash must come from the verified ExecutionClaimed record or the
authenticated base execution-claim index, never from hashing Claim metadata.
Replay now retains that verified anchor as separate unresolved-owner context while folding settlement; independently decoded logical execution values do not supply it.
It must not put a claim record's own hash inside that record's state root,
which would introduce a hash self-reference; the existing separately
authenticated base index supplies the compatibility pattern, and the later
handoff can commit the already fixed original hash safely.
`Store::append_execution` first folds a provisional record; at checkpoint
sequence 10,000, `append_at_with_root` adds root fields that change the actual
record hash. It now reconciles both projected `last_hash` and the unresolved
owner anchor with the final published hash. Verified snapshot open reconstructs
anchors from actual prefix records or authenticated base context, and base
decode attaches its original claim index only after root validation. Anchors
remain outside logical execution bytes and historical roots, are bounded by
the unresolved-owner count, and disappear with settlement. The independent
checkpoint regression verifies final anchors against complete prefix replay,
checkpoint-bound and prefix-reproduced cache paths, and rejects a valid-shaped
provisional handoff hash. Atomic publication now uses that verified context.
An acknowledgement without a declared event creates no handoff; attempted and
interrupted settlements must never create one.

The pure fold must remove a handoff only when an actual accepted event matches
its instance, exact event request key and original event/payload fingerprint.
A disabled event, EventRejected, RequestRejected, another key, changed payload,
read-only refusal, helper exit or missing observation must retain it. An exact
accepted event consumes the handoff in its own atomic replay transition; no
separate cleanup record is needed. Externally cancelled instances must not
authorize fabricated acceptance or erase the durable handoff merely because
the original pending effect is absent.

Handoffs are event-delivery obligations, not execution owners: acknowledgement
still releases the original scheduler slot, and recovery of a handoff cannot
prepare, bind, launch, retry a handler or acquire replacement domain authority.
The collection needs entry and aggregate-byte bounds enforced before append,
with conservative refusal when full and bounded identifier-only health output.

## Format and compatibility review

The historical claim-era boundary is VERSION 11, state-root/4 and base/2;
current writers now use VERSION 12, state-root/5, snapshot/7 and base/3.
Adding the collection to `ExecutionState::to_value` without a new root domain
would silently change historical root/4 bytes, including empty execution
blocks; this approach is rejected. The implementation introduces VERSION 12,
state-root/5 and base/3 to authenticate the complete handoff block,
while preserving byte-identical historical root/4 material and decoding prior
execution/base shapes through explicit historical paths.

Every supported VERSION 1–11 must migrate without rewriting journal records
or inventing handoffs for old acknowledgements. An old sealed fingerprint
cannot establish an original result or event contract; migration must not
claim recovery for that missing material. New base/snapshot roots must include
handoffs even after their original claim, stopped and acknowledgement records
are archived, and later seals must carry exactly the remaining obligations.
Historical record root verification must select its recorded format; new
writes must use the new format. Rewriting earlier hashes or treating an absent
legacy block as proved completion is unacceptable.

## Host integration and proof obligations

Both public tick paths and production startup must discover handoffs from the
same verified read-only snapshot as claims. Applying one requires the healthy
original physical-store writer and matching protected namespace/generation;
a copied identical journal cannot authorize delivery for the original store.
Recovery must use only the durable checked contract and acknowledged result,
with no Root execution request or reconstructed completion invented from a
fingerprint. Parked handoffs retry only after changed journal state, with fair
bounded work alongside ready owners and no duplicate acknowledgement.

The implementation unit must update SPEC, API-POLICY, EMBEDDING and RELEASE
together and include independent cases for each supported prior VERSION,
historical hash-byte preservation, deterministic new roots, forged/oversized
handoffs, torn acknowledgement/event tails and append fault points. Cold
process and actual MCP controls must crash after acknowledgement before event,
restart with removed/changed handlers, and prove one original accepted event
without new allocation or execution, both with live records and after sealing.
Rejected and conflicting event keys must remain unresolved; read-only access,
writer contention and copied physical stores must preserve the obligation.
Zero-dependency, embedding, full portable gates and installed exact-source
native evidence remain cumulative requirements, not substitutes for these
crash and sealed-history cases.

## Store implementation review verdict (2026-10-06)

The bounded StoreState execution_handoffs collection is separate from ownership
and scheduler capacity, sorted by original run and bounded to 4096 entries /
8 MiB. Store publication validates exact original claim hash, stopped result,
request key and next sequence before append; the pure fold validates them
again before the atomic acknowledgement. An actual accepted event retires
only an exact original instance/key/event/send fingerprint and stamped payload.
Both live note_record and replay apply that transition, preventing checkpoint
roots or live state from retaining an already accepted obligation.

Base/3 and snapshot/7 carry obligations after their consumed claims disappear;
explicit base/1 and base/2 decoding retains original root/3 and root/4 bytes
and refuses inserted handoff blocks. The new empty-collection goldens were
derived with Python SHA-256 from independently verified historical root
material; the snapshot self-hash uses its specified empty hash slot. Actual
store regressions prove cache/cold/two-seal retention, exact-event retirement,
foreign-key and foreign-anchor refusal, torn ack/event repair, and real
write/rotation/fsync failures retaining the stopped owner until reopen.

Initial validation found an external core-only acceptance test incorrectly
referencing the store crate, stale snapshot hash domains in hostile fixtures,
an independent snapshot derivation missing the empty hash slot, and a future
VERSION 12 refusal vector made current by this bump. These were corrected by
placing the store visibility/refusal proof in its external integration target,
using the declared current domain without weakening semantic assertions,
independently verifying the historical empty-slot recipe, and retaining the
future-version refusal at VERSION 13. Targeted MSRV checks now pass; full
stable/portable/native format gates remain pending, and no installed proof
is inferred from preauthenticated store fixtures.

Cold NativeOwners now adopts bounded event-only handoffs separately from
unresolved claims and checks original physical store, namespace/generation
and protected authority device/inode before delivery, without new execution
authority. Genuine installed cold process/MCP and sealed acceptance,
production defaults, shutdown and complete crash/concurrency axes remain
necessary before task 9401 can be completed.

## Cold host implementation review — 2026-10-06

Source inspection identifies the integration points for the remaining work;
this section records the proposed implementation, not completed capability.
`service::plan` obtains one verified read-only snapshot and invokes
`Runner::recover_native_owners` before scheduler observation; both public tick
paths share it. `settle_phase` invokes `apply_native` under the actual writer.
Handoff adoption and event application belong at these same points.

Discovery currently verifies the root-protected store registration against the
physical directory device/inode, pins the selected authority directory while
checking the broker route, and then returns only namespace/generation.
`check_claim_store` consequently omits the original authority device/inode.
The retained claim domain already contains this identity, and
`containment::closing::recorded_domain` checks it against the authority
metadata. Discovery should retain that verified identity internally and check
it together with namespace/generation before cold delivery; a recreated
same-path authority must not acquire the original event permission.
This lookup must remain read-only and make no prepare/bind/execute request.

The host should retain bounded event-only entries keyed by original run ID,
separate from NativeExecution owners and admission capacity, with an original
handoff value and last attempted journal sequence. Adoption copies only the
verified snapshot collection and validates the original contract using
HandlerSpec::from_contract and the claim fingerprint. Missing observations
cannot alone erase a retained entry; under the healthy original writer, exact
current durable collection membership decides whether it remains actionable.
A changed original value is a refusal rather than replacement permission.

The writer path should recheck physical identity and protected authority,
require exact durable handoff membership, recover the original on_ok/on_failed
advance from the contract and actual outcome, and send under the recorded
derived event key without acknowledging again. It must not fabricate a
NativeCompletion, reconstruct a closure receipt, look up the current handler
table, or launch a helper. Acceptance is established by the resulting durable
collection retirement, not merely a successful send response: ignored,
rejected and conflicting-key results must preserve and park the obligation.
Disabled or cancelled instances likewise retain it without burning the key.

Apply at most one ready native work item per tick and alternate ready owner
and handoff work so neither can starve; rotate handoff selection by original
run ID, retry parked entries only after observed journal progress, and retire
no scheduler claim slot for event-only delivery. Required installed controls
remain cold process and MCP completion, changed/removed handler tables,
two-seal restarts, copied/replaced store and authority identities, read-only
and contended writers, disabled/rejected events, and foreign-key conflicts.

## Applied cold host unit — acceptance pending

The reviewed host design is now implemented in NativeHandoffs, NativeOwners,
Pipeline and protected discovery. Retained handoffs have both a 4096-entry
limit and an 8 MiB canonical aggregate limit; adoption refusals remain visible
without blocking original-writer reconciliation of already retained work.
A warm completion keeps its original retry path, while cold work uses no
NativeCompletion or execution capacity. Stable/MSRV all-target Clippy compiles
the new real installed controls; 28 library tests, native-mode refusal and
all 12 existing serve_modes regressions pass locally. These are not installed
cold acceptance results. The provisioned broker matrix now includes cold
process/MCP axes which discard original host state and tables, retain disabled
event obligations, reopen across two verified seals, refuse copied stores and
read-only/contended writers without journal mutation, and assert no extra
allocation before original accepted-event delivery. Production constructors
remain legacy until installed acceptance and the other lifecycle obligations
are proved.

### Retention control review — 2026-10-06

The provisioned broker case now separately drives process/MCP conflicts and
rejected-original-key recovery using actual journal records rather than
metadata-only fixtures. The conflict case requires an accepted foreign
EventApplied under the original derived key and request_id_conflict on
original delivery; the rejection case requires EventRejected before cold
restart and a parked/refused delivery afterward. Both assert full handoff
equality, no repeated-tick append, zero unresolved execution claims and
cold-reopen retention. Two seals and physical-copy/read-only/contention
controls run before the final original-writer refusal. The root wrapper
requires each dedicated child marker and checks the unchanged original
binding and allocation count. Existing successful controls remain intact.
The enum moved to a dedicated test module to keep the broker source below
1,000 lines. Formatting, source-size and both toolchain all-target Clippy
checks passed; installed execution and exact-source acceptance remain
pending, and no new runtime capability or production default is claimed.

### Destructor boundary review — 2026-10-06

Corrected Runner's stale legacy-only destructor documentation: direct-child
signal survival and missing adoption apply to the legacy children map;
native claim recovery and retained event handoffs use their separate paths.
Dropping a native transport still supplies no verified closure or durable
settlement, and Drop supplies no bounded drain/abort guarantee. This is a
comment-only correction with no changed executable behavior or disk bytes;
task 9402 remains planned and its production quiet-stdio control proof is
still required. Diff checks cover the committed correction; execution gates
are omitted for this documentation-only unit.
