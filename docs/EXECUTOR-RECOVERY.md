# Recovering an interrupted native executor

These commands apply to the provisioned Linux/systemd containment backend.
Use the original physical data directory and its configured operator account;
a copied store or a replacement authority cannot prove the original tree closed.
Run inspection is read-only and can coexist with the original writer.

## Upgrade with an original run still retained

Inspect before replacing the executor or changing its handler table:

```sh
fsm --json --data-dir "$data_directory" execute runs
```

Select `run_id` from `runs`; `phase` is journal state and
`native_evidence: "unverified"` does not establish whether a process is alive.
Keep the original protected authority, handler catalogue, receipts, journal and
request ledger until original settlement and event delivery finish.

If the original executor is responsive, request bounded drain through its
original control directory:

```sh
fsm --json --data-dir "$data_directory" execute stop \
  --control-dir "$control_directory" --mode drain --timeout-ms 8000
fsm --json --data-dir "$data_directory" execute runs
```

A refusal or timeout leaves ownership retained; it is not permission to start
another tree. Native termination signals use the supervisor's lease-EOF cleanup
policy rather than graceful drain, and executor exit alone is not closure proof.

## Reconcile the original interrupted run

After the original owner has retired, select its exact recorded run ID:

```sh
fsm --json --data-dir "$data_directory" execute reconcile \
  --run-id "$run_id" --timeout-ms 8000
fsm --json --data-dir "$data_directory" execute runs
fsm --json --data-dir "$data_directory" execute reconcile \
  --run-id "$run_id" --timeout-ms 8000
```

For an orphan without an original published result, successful authenticated
closure writes the original stopped and interrupted settlement records before
retry becomes eligible; it preserves the pending effect and fabricates no event.
The response's `execution.run_id` identifies the original run and
`execution.disposition` is `"interrupted"` in this case.
The repeated command returns `duplicate: true` through the original request
ledger, without another append or a native request targeting a successor.

An original completed publication follows authenticated result recovery instead;
its original outcome and contract determine settlement, even if current handlers
were removed or changed. Reconciliation does not deliver its outcome event:
configured startup recovery delivers retained acknowledgement handoffs once.
An empty `runs` array therefore does not imply all event handoffs have finished;
check `execution_ownership.outstanding_handoffs` as well.

## When recovery refuses

- `store/lock`: another writer owns the original store; let that owner release
  it before retrying, while inspection remains available.
- Active runner lease: use the original executor's bounded stop control and
  wait for its authenticated retirement before reconciling.
- Missing runner lease or binding before entry: retain the claim and original
  authority; pre-run owner recovery is incomplete, so do not create replacement
  ownership files or treat an empty domain as evidence.
- Partial original result: preserve its bytes and original authority for
  authenticated result recovery; closure-only reconciliation refuses them.
- Changed physical store, boot, authority or domain identity, or missing native
  facilities: preserve the original state and restore the original facilities
  where possible; no environment-reset clearance is currently promised.
- Missing historical claim or request ledger: exact settlement replay refuses;
  retain original verified history rather than manufacturing a new request ID.

These local commands do not reconcile a remote handler's external side effects;
use the workflow's declared remote reconciliation or compensation contract.
