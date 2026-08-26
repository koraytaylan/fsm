# Plan 0012 — Live MCP Surface — 🚧 In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this file. Task-level truth lives in [tasks/](tasks/) frontmatter; Makina's integration coordinator updates both layers.

- **Status:** 🚧 In progress.

- **Goal:** a server that can speak first — instances addressable as resources, subscriptions that push `notifications/resources/updated` when a workflow advances, list-changed notifications, structured logging, progress on long calls, and cancellation that is honest about what it can interrupt.
- **Root cause:** every `listChanged` is false, `subscribe` is false, there is no logging capability, cancellation is written to stderr and discarded, and the loop blocks on the client — so an engine whose whole premise is that things happen while nobody is watching can only be polled; and instances, the live objects, have no URI to watch in the first place.
- **Approach:** one output multiplexer holding a mutex across whole-line writes, so a background thread and the request path can share stdout without ever interleaving bytes; a change feed that polls `Store::open_read_only` on the executor's own 250 ms cadence, compares one integer in the common case, and maps only the new records to the subscribed URIs they touch; a thread spawned only when a session actually subscribes, so the plan is inert for callers that do not use it; and a documented, deliberate limit that a single tool call is not interruptible mid-step, because threading cancellation through the pure core would cost the core its purity and buy nothing.
- **Progress:** 5/14 tasks done; 0 blocked; 0 dropped.
- **Integration:** `planned`; run —; base `develop` @ `6f690a97a2a10c7b355db09e88c2383753b21842`; validation base —; mode —; final integration —.
- **Exceptions:** — (coordinator-owned blocked/dropped reasons are recorded here).
- **Outcome:** A model subscribes to a workflow it started and is told when it moves, instead of asking every few seconds whether anything happened — and the README's claim about watching a store live becomes true.

_Task frontmatter is authoritative; this file is the roll-up._
