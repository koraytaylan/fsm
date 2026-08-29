# Plan 0017 — Journal Lifecycle — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** let an operator seal a prefix of the journal into an immutable archive and detach it, so disk and open cost track the retention window instead of the store's lifetime — without losing a single claim the engine currently makes about that history.
- **Root cause:** the journal is append-only by design and nothing bounds it. Disk tracks lifetime rather than workload, a cold open folds every record ever written, and `journal verify` — the strongest claim the project makes — is O(all history) permanently, which is exactly the incentive that stops an operator running it.
- **Approach:** promote the anchor that already exists. A `state_checkpoint` is a hash-chained record carrying an authoritative `state_root`, so sealing cuts only at a checkpoint and needs no new trust anchor. A `journal_sealed` record appended at `N+1` chains to record `N` by construction and commits the base's two roots; the base state becomes a required `fsm.base/1` file, distinct from the disposable snapshot caches it resembles; and request fingerprints — which `state_root_at` deliberately excludes because the claiming record authenticates them — are carried in the base under a **new additive** domain, so **no historical root moves**. A dedup entry may be dropped only when replaying it is already impossible for an independent reason, and an unsafe cut is refused with `store/archive_refused` rather than degraded into a weaker guarantee. Two rules govern what a cut may take: keys belonging to live instances are carried whatever their sequence, and a **pending effect pins the archive**, because the executor re-derives an effect's arguments and attempt count by scanning records rather than reading state.
- **Progress:** 16/16 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `47d35a241c39e2c1ffad648dd68f023a62ec0fb1`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** a store that has run for a year opens in the time its retention window costs, `journal verify` stays affordable enough to keep running, and the sealed bytes are a directory an auditor can check with `sha256sum` without installing anything.

_Task frontmatter is authoritative; this file is the roll-up._
