---
id: 0017
title: "Journal Lifecycle"
status: planned
---
# Scope — Plan 0017

> A journal that only grows is a journal with an expiry date nobody wrote down.

## Why this plan

Every guarantee in the README is about what the store *keeps*. None of them is about what it costs to keep it. `SPEC.md` states the position plainly — "Interior history is never rewritten. Snapshots are disposable caches, never authoritative, never part of the chain" — and that position is correct for integrity and incomplete for operation. There is no compaction, no retention, no archival, and no export. The consequences compound quietly:

- **Disk tracks lifetime, not workload.** A store that has run a year holds every record of that year, whether or not a single instance from month one still exists. There is no supported way to move those bytes anywhere, and no unsupported one either — deleting a segment breaks the chain and the store refuses to open.
- **Cold open cost is unbounded.** `load_records` walks every segment and `fold_with` folds every record. Snapshot caches accelerate this, but they are caches: `SPEC.md` requires that any cache be re-derivable, so the authoritative path remains a complete fold, and a corrupted or absent cache silently falls back to it.
- **`journal verify` cannot get cheaper.** It is the strongest claim the project makes and it is O(all history), permanently. An operator who runs it weekly pays more for it every week, which is precisely the incentive that stops people running it.

The fix is not to weaken the chain. It is to let an operator **seal** a prefix — prove its end state, move its bytes to an archive, and keep a record in the live chain that says exactly what was sealed and what it hashed to. After a seal, disk and open cost track the retention window instead of the lifetime, and every claim the store made about the sealed prefix is still checkable: with the archive present, by walking it; without the archive, by checking that the seal's committed hashes match the base the store is running on.

## What sealing must not quietly change

The hard part of this plan is not the seal. It is that several live facts are **not** in the folded state at all — they are re-derived by scanning records — so archiving records changes what the system concludes without changing anything it reports. Every one of these was found by walking the code rather than by reasoning about the design, and each has its own task:

| Reader | Scans for | What sealing would do |
|---|---|---|
| `execute/effect.rs` | the emitting record, by sequence | the effect never resolves, so it never runs and never fails |
| `execute/effect.rs::fold_before` | every record below the emitting one | folds from empty, so a handler runs against **stale arguments** |
| `execute/watch.rs::attempt_state` | every `effect_attempted` | the attempt count falls, so an exhausted effect retries forever |
| `execute/dead.rs` | failed acks | the dead-letter report silently shrinks |
| `store/view.rs::history_page` | records for an instance | a partial history is presented as a complete one |
| `store/view.rs::explain_seq` | a record by sequence | reports "no such sequence" for a record that exists, in the archive |
| `snapshot/files.rs` | creation records | writes a cache claiming fewer machines and instances than the store has |

Two answers cover all seven. Where the records can be spared, the **pin** refuses to archive them. Where they cannot, the reader learns to fold from the base or to report the seal. Nothing here is allowed to end in a smaller answer that looks like a complete one.

## The three rules this plan must not break

- **The chain stays unbroken.** The seal is a record, appended in the ordinary way, whose `prev_hash` is the hash of the last sealed record by construction. Sealing adds a record; it never rewrites, reorders, or removes one from the live chain.
- **Nothing authoritative is a cache, and nothing cached becomes authoritative.** The base state a sealed store opens from is a *new, required* file with its own format — not a promoted snapshot. Snapshot caches keep their exact current meaning: disposable, re-derivable, skipped when stale. The two must not be confused, because a required file that behaves like a cache is a store that opens onto guessed state.
- **No guarantee is lost by sealing, and any that cannot be kept is refused rather than degraded.** This is the rule that shapes the whole design, and §0079 is where it bites: idempotency is the one property that lives in a table rather than in the records. Where a guarantee can be preserved by carrying something forward, it is carried; where it cannot, the cut is refused. It is never quietly narrowed.

## The idempotency problem, and why the answer is to carry rather than expire

`state_root_at` in `crates/fsm-core/src/replay/mod.rs` carries a comment that is the crux of this plan:

> Only the claiming seq enters the root, not the request fingerprint. The fingerprint lives in the record body that claimed the key, so the hash chain already authenticates it; including it here would change every historical root for no added binding.

That reasoning is correct today and stops being correct the moment a claiming record leaves the live journal. Two things follow, and they are separate:

1. **Surviving keys need their fingerprints carried.** A `request_id` claimed at or below the seal whose entry must survive can no longer have its fingerprint re-read from the live chain. The base state carries it, and the seal commits a hash over those fingerprints under a **new, additive** domain — so nothing authenticates less than before and **no historical root moves**.
2. **Dropped keys must be impossible to replay wrongly.** A dedup entry that is dropped cannot be distinguished later from a `request_id` that was never seen; there is no honest way to report "this one expired". So the plan does not try. A seal **carries** every key belonging to a live instance — whatever its sequence — along with every key claimed above the cut, and drops only the rest: keys belonging to settled instances, and keys belonging to no instance at all. Each of those is already unreplayable for an independent reason, and §0079 of the architecture states all three and requires each to be asserted against a real store rather than argued.

The bound that produces is the correct one: **carried keys track live workload, not lifetime.** A store with a thousand finished instances and three running ones carries three instances' keys. A seal is refused only when what must be carried does not fit a base file, with a hint naming the two things that clear it.

This is why the plan introduces no expiry error and no "replayable but not conflict-checkable" second class. The first shape of this rule — drop everything below the cut, refuse if any of it belongs to a live instance — was discarded during review for a decisive reason: a cut near the head puts almost every key below it, so that rule would have refused essentially every seal a running store could ask for.

## In scope

- **0079 — The seal and the base.** The `journal_sealed` record kind and every exhaustive site that must learn it; the `fsm.base/1` authoritative base-state file with the additive `fsm:base-dedup:1` fingerprint domain; the carry rule for idempotency keys; and the **pin** — the lowest sequence any live derivation still depends on, because the executor recovers a pending effect's arguments and attempt count by scanning records rather than reading state.
- **0080 — The archive.** The `fsm.archive/1` manifest with per-segment hashes and chain endpoints; the crash-safe operation that writes the seal and moves the segments in an order every interruption leaves recoverable; and store `VERSION` 10 with a migration matrix covering every version still supported.
- **0081 — Reading a sealed store.** Opening by folding the live suffix onto the base; verification that reports a sealed prefix as a distinct verdict and never as a silent pass, plus `--with-archive` for the complete walk; replay and doctor teaching the same truth; the executor's three record scans made correct; and the three remaining readers that would otherwise return a smaller answer without saying so — instance history, `explain`, and the snapshot writer's own view of which machines and instances exist.
- **0082 — Surface.** `fsm journal archive` as the only way to seal, and the existing audit tools reporting a store's seal without adding a tool.
- **0083 — Proof and docs.** A crash harness that interrupts a seal at every step and asserts the store folds afterwards, and the operator documentation for the whole lifecycle.

## Out of scope

Automatic or scheduled archival. Sealing is an explicit operator command with an explicit cut point, for the same reason a deadline fires only when a caller polls: a store that reorganizes itself on a timer has a background writer, and this engine does not have one.

Deleting archived bytes. `--to` is mandatory and the operation moves segments; destroying them stays a separate act an operator takes with their own tools, as `repair --truncate-torn-tail` already is.

Rewriting, compacting, or summarizing interior records. A sealed prefix is the same bytes it always was, relocated. Nothing is rewritten into a denser form, because a record that is not the record that was written is not evidence.

Sealing across a live instance's keys, distributed or remote archives, archive encryption, and any retention policy expressed as a duration rather than a sequence. Time-based retention needs a clock the store does not read; the cut point is a sequence, and an operator who wants a date converts it with `instance history` first.
