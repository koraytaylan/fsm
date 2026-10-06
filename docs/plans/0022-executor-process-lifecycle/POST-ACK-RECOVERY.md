# Post-ack recovery implementation review

Status: implementation prerequisite reviewed; not implemented or accepted.
This review preserves task 9401's full production and crash-recovery scope.

## Observed gap

`ExecutionState::settle(Acked)` removes the original claim and retry ledger.
`apply_settled` validates the recorded result against the original stopped
result and applies acknowledgement semantics in that same record, but retains
no outcome-event handoff. `NativeOwners::Owner` retains the checked original
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
provisional handoff hash; atomic handoff publication remains unimplemented.
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

The current persistent boundary is VERSION 11, state-root/4 and base/2.
Adding the collection to `ExecutionState::to_value` without a new root domain
would silently change historical root/4 bytes, including empty execution
blocks; this approach is rejected. Implementation must introduce VERSION 12,
state-root/5 and a base format that authenticates the complete handoff block,
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
