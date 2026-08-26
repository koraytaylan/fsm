---
id: subscription-registry
title: "Subscription Registry"
workstream: "0059"
kind: task
depends_on:
  - session-shutdown
  - instance-resources
gated: false
touches:
  - crates/fsm-cli/src/mcp/subscribe.rs
  - crates/fsm-cli/tests/mcp_subscribe.rs
status: planned
merged_as: ""
---
# Subscription Registry

Subscriptions are per session and capped, and the first one is what brings the change feed to life — so the registry owns both the set and the decision to spawn.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/subscribe.rs` holding `pub struct Subscriptions { uris: Arc<Mutex<BTreeSet<String>>> }` with `subscribe`, `unsubscribe`, `contains`, `len`, and `snapshot` for the feed thread to read without holding the lock across its work.
2. Fill the `resources/subscribe` and `resources/unsubscribe` bodies `5702` already routed to this module. Both take `{uri}` and return an empty result object on success. The routing exists; do **not** edit `serve.rs`.
3. Refuse a URI the server does not serve with `-32002`, the same code `resources/read` uses for the same reason. Validate against the actual resource resolver rather than a prefix match, so a subscription can never name something unreadable.
4. Make both operations idempotent: subscribing twice succeeds, unsubscribing something not subscribed succeeds. The client's intent is satisfied either way, and an error would only invite retry loops.
5. Cap at `MAX_SUBSCRIPTIONS = 64` per session, refusing beyond it with `INVALID_PARAMS` and a hint naming the cap. An unbounded set is an unbounded per-poll cost, and this is the only backpressure the design has.
6. Spawn the change feed on the **first successful** subscription, per `5703`'s lazy rule, and hand it a clone of the `Arc` and a clone of the `Notifier`. Do not stop the feed when the last subscription is removed — a session that unsubscribes and resubscribes is common, and a thread that parks on an unchanged `last_seq` costs one integer comparison per interval.
7. Keep subscriptions per session and say so in the module doc: a second client connecting to a second `fsm serve` process shares no state with the first, which is exactly right for stdio and is the thing plan 0015 will have to revisit for a shared transport.

**Tests:**

- `crates/fsm-cli/tests/mcp_subscribe.rs`: subscribing to a valid instance URI returns an empty result and registers the URI.
- Subscribing to `fsm://machine/{id}` and to the documentation URIs succeeds; subscribing to an unknown instance, a malformed URI, or an unserved path returns `-32002`.
- Subscribing twice returns success and leaves one entry; unsubscribing an unsubscribed URI returns success.
- The 65th subscription is refused with `INVALID_PARAMS` and a hint naming 64; the 64th succeeds.
- The feed thread is spawned on the first successful subscription and not before — assert against the spawn counter `5703` introduced.
- Unsubscribing the last URI does not stop the feed.
- Two sequential sessions have independent subscription sets.
- A subscription made before `notifications/initialized` still works, matching the leniency the loop already applies to other methods.

- **Done when:** `cargo test -p fsm-cli --test mcp_subscribe` passes every case above including idempotency, the cap, and lazy spawn, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
