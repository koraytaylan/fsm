---
id: 0014
title: "Audit Surface"
status: planned
---
# Scope — Plan 0014

> A model operating this engine cannot verify the hash chain it is being told is tamper-evident.

## Why this plan

The CLI has five capabilities the MCP surface does not: `explain`, `journal verify`, `journal replay`, `doctor`, and `instance annotate`. That asymmetry is backwards for a system whose primary operator is a model.

- **`explain` is the best diagnostic affordance in the system and it is unreachable.** `store/view.rs::explain_seq` reconstructs the complete decision trace for a journaled step — every candidate, every guard verdict, every block, every set with its before and after. It is exactly what a model needs when a workflow did something surprising, and there is no tool for it.
- **The audit posture cannot be audited.** The README lists "tamper-evident history — hash-chained, fsynced records" as a guarantee. `journal verify` is what checks it. A model can be told the chain is intact and has no way to check, which makes the guarantee something it takes on faith from the thing making the claim.
- **`replay` proves determinism and is CLI-only.** "Replay determinism" is a headline property; the operation that demonstrates it is not in the surface.
- **`doctor` reports store health and is CLI-only.** Worse, when a store is genuinely unhealthy the MCP server does not start at all: `serve_dir_with` writes one line to stderr and returns `Err`. The model's server disappears with a message the user may never see, at exactly the moment diagnosis is most needed.
- **The `annotated` record kind exists and nothing can write one.** SPEC lists it in `### Record kinds`; `fsm instance annotate` writes it; no MCP tool does. A model producing an audit trail cannot leave a note in it — cannot record why it cancelled something, or what a human said on the phone, or which ticket this instance corresponds to.

This plan closes all five, and adds the mode that makes the fourth one useful: a server whose store will not open **still starts**, serving the diagnostic tools and the documentation resources, and answering every other tool with a clear error naming the health and the remedy. Diagnosis is precisely the case where the server must not vanish.

One capability is deliberately **not** exposed, and the reasoning is part of the plan rather than an omission. `fsm repair --truncate-torn-tail` quarantines bytes and truncates a journal. It is the one operation in this system that destroys data, it is correct only after a human has looked at the quarantined bytes, and its safety argument rests on an operator understanding what a torn tail is. The audit tools tell a model exactly what is wrong and print the command a human should run. They do not run it.

## In scope

- **0066 — Read-side audit tools.** `explain_step`, `journal_verify`, `journal_replay`, and `store_doctor`, all read-only and therefore all available on a `--read-only` server. `journal_verify` and `journal_replay` are the first genuine consumers of plan 0012's progress notifications and cancellation, because they are the only operations here whose cost scales with journal length.
- **0067 — Degraded serve mode.** A server that starts against an unopenable store, advertises the same tool list, answers the diagnostic tools from a read-only classification, and refuses everything else with the store's health, the blast radius, and the remedy command — instead of exiting before the client ever connects.
- **0068 — Write-side and proof.** `instance_annotate`, the one mutating tool in this plan; a golden session covering every audit tool against both a healthy and a deliberately corrupted store; and the documentation of what each tool proves and what the surface deliberately will not do.

## Out of scope

`repair` in any form, for the reason above — the tools name the command, a human runs it. Any change to the verification algorithm, the health classification, or the recovery posture: this plan exposes `journal_io/verify.rs` and `journal_io/classify.rs`, it does not modify what they conclude. Lock contention, which is a different reason a store may be unavailable and belongs to plan 0015 — that plan reuses this one's degraded mode rather than inventing a second one. Any new record kind: `annotated` already exists and this plan writes it through the store method that already exists.
