---
id: list-changed-notifications
title: "List Changed Notifications"
workstream: "0059"
kind: task
depends_on:
  - journal-change-feed
gated: false
touches:
  - crates/fsm-cli/src/mcp/subscribe.rs
  - crates/fsm-cli/tests/mcp_list_changed.rs
  - crates/fsm-cli/src/mcp/watch.rs
  - crates/fsm-cli/tests/mcp_list_changed.rs
  - crates/fsm-cli/tests/mcp_change_feed.rs
status: done
merged_as: ""
---
# List Changed Notifications

`listChanged` is a capability rather than a per-resource subscription, so this notification fires for any session that negotiated it — and firing once per poll batch instead of once per record is what keeps it useful rather than noisy.

**Steps:**

1. In `crates/fsm-cli/src/mcp/subscribe.rs`, add the list-changed decision beside the subscription set: a poll batch containing at least one record that adds a **machine or an instance to the listing** emits `notifications/resources/list_changed` **once**, regardless of how many appeared. That set is `machine_defined`, `instance_created`, and — because plan 0010's fold derives a child instance from it rather than writing a separate creation record — `instance_invoked`. A child that appears in `resources/list` without a `list_changed` is a listing a client never re-reads.
2. Emit it independently of any subscription. A session that negotiated `resources.listChanged: true` gets it whether or not it subscribed to anything, because the notification is about the *listing* and not about a resource.
3. Emit it **after** the batch's `resources/updated` notifications, so a client that reacts by re-listing sees a listing consistent with the updates it was just told about.
4. Do not emit it for records that only advance an existing instance — `event_applied`, `deadline_applied`, `effect_acked`, and the rest leave the listing's membership unchanged, and a client that re-lists on every transition would be worse off than one that polls.
5. Emit no `notifications/tools/list_changed` and no `notifications/prompts/list_changed`. Both capabilities are `false` and both sets are static; sending a notification the server did not advertise is a protocol error, not a courtesy.
6. Share the feed's batch walk rather than re-reading records: the feed already has the new records in hand, and a second read would be both slower and capable of disagreeing with the first.

**Tests:**

- `crates/fsm-cli/tests/mcp_list_changed.rs`: a poll batch containing one `instance_created` emits exactly one `notifications/resources/list_changed`.
- A batch containing three `instance_created` and two `machine_defined` records emits exactly one.
- A batch containing only an `instance_invoked` emits exactly one, since the child joins the listing.
- A batch containing only `event_applied` and `effect_acked` records emits none.
- The notification is emitted for a session with **no** subscriptions.
- Ordering: in a batch that both creates an instance and advances a subscribed one, the `resources/updated` line precedes the `list_changed` line.
- No `tools/list_changed` or `prompts/list_changed` is ever emitted — assert across a session exercising every tool.
- The notification carries no params beyond what the specification requires, and byte-matches its golden.
- A session whose client negotiated an older protocol version still receives it, since the capability exists in all three accepted revisions.

- **Done when:** `cargo test -p fsm-cli --test mcp_list_changed` passes every case above including the once-per-batch rule and the ordering guarantee, no unadvertised notification is ever sent, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `subscribe::changes_the_listing` as the one membership rule, emitted from the feed's existing walk after the batch's updates, and the suite — one creation, a five-joiner batch, an invoked child, movement alone, the ordering, the two notifications that are never sent, and the notification's exact bytes. Three assertions in `5902`'s suite moved from counting notifications to naming the URIs reported, since a batch that creates something now carries one more line than it did — the URIs are what those tests were ever about.
