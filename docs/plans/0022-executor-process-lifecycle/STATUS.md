# Plan 0022 — Executor Process Lifecycle — In progress

Task frontmatter is authoritative; the coordinator owns registration and completion.

| Task | Status | Landing OID |
|---|---|---|
| [lifecycle-containment-feasibility](tasks/9301-lifecycle-containment-feasibility.md) | done | 399ed6ed5636118151ffb7ad94140538f865d1a7 |
| [durable-execution-claims](tasks/9302-durable-execution-claims.md) | done | cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb |
| [contained-handler-runner](tasks/9303-contained-handler-runner.md) | done | 9f1f175ad91609359699e3a2d670119e8cbb506a |
| [executor-ownership-integration](tasks/9401-executor-ownership-integration.md) | done | 2b580fd762d9afe54f844e18514387d72b6c6bd2 |
| [bounded-executor-shutdown](tasks/9402-bounded-executor-shutdown.md) | done | 5730f17202cdeabd8c34f9b1c48fcf02f26b0e06 |
| [uncertain-run-reconciliation](tasks/9403-uncertain-run-reconciliation.md) | done | 713c90e92871efb3484b41fdd20ee211dd196569 |
| [lifecycle-crash-matrix](tasks/9404-lifecycle-crash-matrix.md) | in_progress | — |

Progress: 6/7 tasks completed.

Frozen `0211231c..6e7f0321` independently verifies all sixteen executor-crash
cases on Linux stable/MSRV in [CI 37822729330](https://github.com/koraytaylan/fsm/actions/runs/37822729330),
including quiet/noisy pre-publication, collected-timeout and successful
process/MCP candidates through standalone and embedded hosts; verdict digest
`1150783cd57801b05d93f6036964335a228b66ed096e2395afe261968bd82029`.
Each compiler also passes 82 containment cases, 34 production workflow scenarios
and two historical upgrade scenarios, with matched report/log/transcript
hashes; executable bytes were not independently compared. Successful candidates
recover their original completion without another claim or handler entry.
Earlier failed checkpoint `d1b763e5` remains retained with verdict
`cdd89d23c56f9b51759340b6fdb4e21de8fd1028f5686fb6d64afbe6d63e4886`;
its stable liveness-response failure has no proved cause. Frozen
`6e7f0321..7767ffb0` independently verifies the strengthened live-tree restart
barrier in all sixteen cases on both compilers in
[CI 37824386466](https://github.com/koraytaylan/fsm/actions/runs/37824386466);
scoped crash-verdict digest
`eac41bb8f6632e3016bf555dd4332adaeeebd083de1dc8aeddba15df3ad5101b`.
The frozen-source crash verifier rejects nine altered-evidence controls;
broader reports from this successor have not been independently verified.
Supervisor death, remaining crash boundaries, repeated-run resource use,
sensitivities and current full portable checkpoint remain unverified;
9404 stays in progress, and original artifacts remain in the task cache.

Frozen `8ac4d8d0..932ac99e` fails its forty-case native checkpoint on both
compilers: the standalone/process after-claim observer passes, but Root
verification wrongly requires a launched-domain memory receipt for the
unlaunched original allocation; retained failure-verdict digest
`54f2810b1e3654957547f7e5f95244660a05aaf160a71bd455af88ed5a1491b2`.
Frozen `932ac99e..2be0109b` independently verifies all forty crash cases on
Linux stable/MSRV in [CI 37831436777](https://github.com/koraytaylan/fsm/actions/runs/37831436777);
scoped crash-verdict digest
`79fdf539cbc2b41011e62eb5aa61a48fff7dc47c874388f7c0344b5f1125d6e0`.
The correction matches the original repair binding, proves absent launch
records and preserves launched-successor memory checks. The inventory covers
supervisor death, verified domain closure, stopped/ack/event publication and
pre-launch durable claims across both hosts and handler kinds. Successor
`2be0109b..8237c9b4` independently verifies all forty-four crash cases and both
82-case containment inventories in
[CI 37832992442](https://github.com/koraytaylan/fsm/actions/runs/37832992442);
scoped verdict digest
`9b9ae178dc1e76e84422b7478607c5ac4c373c67c0032edfdee8373c4e899ed8`.
The added authorization boundary pins the real enrolled gate before grant,
proves no handler entry, excludes a competing restart and permits sequential
recovery only after matched closure. Workflow/upgrade reports and executable
bytes from this successor were not independently verified; remaining
spawn/termination boundaries, repeated-run resource use, guard sensitivities
and current full portable integration remain unverified, so 9404 stays in
progress. This documentation-only update omits heavy gates.

Frozen `8237c9b4..d5579943` independently verifies 48 crash cases, all 48
resource observations and 82 containment cases per compiler on Linux
stable/MSRV in [CI 37835587176](https://github.com/koraytaylan/fsm/actions/runs/37835587176);
scoped verdict digest
`1b8edf7a1e024e4147a649d62ab50d8acd01e23bc2f48045a861d5c761ef4796`.
Twelve sequential noisy descendant trees run in each original host/handler
pair, with prior matched closure and no live overlap; retained measurements
meet the finite descriptor, thread and RSS bounds. Workflow/upgrade reports
and executable bytes are not independently verified by this verdict.
The separate `e7bf5f5f` integration checkpoint fails MSRV embedded historical
upgrade recovery after original drain: its successor retains run 2 without
launch or stopped evidence; cause remains unproven, and the passing successor
checkpoint establishes no behavioral fix. Remaining crash/termination axes,
complete guard sensitivity and current full portable integration keep 9404
in progress; artifacts stay in the task cache, and this docs update omits
heavy gates.

Frozen `d5579943..4ffda761` independently verifies both Linux compilers'
48 crash cases and resource observations, 82 containment cases, 34 ordinary
workflow scenarios and two historical upgrade scenarios in
[CI 37838320938](https://github.com/koraytaylan/fsm/actions/runs/37838320938);
scoped verdict digest
`ec22523aaa0aee016fdd83b85496ae069211708f2a80d4951e26854329f5a6a3`.
Actual retained/stop/drained and refusal/interruption/duplicate transcripts
match retained logs and frozen inventories; executable bytes and portable
full integration are not independently verified. Passing historical recovery
establishes no cause or behavioral fix for the earlier failed checkpoint.

Frozen `2ccf68dc..30ca43a5` independently verifies claim-before-start
sensitivity at all six shared-check callers: binding, selected-group grant,
exec-status listener, launch, runner and enrolled grant after cancellation.
Each named case passes, fails at exit 101 after only the shared durable-claim
check is neutralized, then passes after exact restoration on Linux stable/MSRV;
consolidated scoped verdict digest
`e6ff9267130aa5a21a2d35ade38614e94def95171c0d9c968e3b5a570cb4d3b0`.
The deliberately failed fixtures retain authorities on disposable CI and grant
no environment-reset clearance. Successors `b6a57d44` and `422a5f0f` include
these six cases in the full containment inventory and admit only canonical
nested names in its verifier; successor inventory execution, identity/closure
sensitivities, remaining crash/termination axes and current full integration
remain unverified, so 9404 stays in progress. These documentation-only
milestones omit heavy gates; original evidence stays in the task cache.

Frozen `5730f172..713c90e9` completes 9403 against its unchanged acceptance inventory;
final review digest `22c2aa44638766b727dc041bfd62d914fc26365d69288454e5c2df347502211a`.
The six portable gates, dependency/core-only check and both verified native
matrices pass at `c7fc9e8d`; subsequent changes only correct documentation and
record the frozen verdict. Production process/MCP orphan closure, original
owner exclusion, concurrent/idempotent reconciliation, stale-run isolation,
actual PID reuse and copied/changed/missing identity refusals are verified,
with stopped publication before retry and preserved original results. Both
compilers retain interruption/replay and historical executor upgrade/drain
transcripts. The selected backend grants no environment-reset clearance;
missing evidence retains ownership. The earlier stable successor stall remains
preserved with cause unproven; subsequent passes establish no behavioral fix.
The complete 9404 crash inventory and non-Linux native capability remain
unimplemented. This documentation-only completion passes diff and size checks
and omits heavy gates; detailed evidence remains in the task cache.

Frozen `5730f172..fd5affe2` passes all six portable full gates, the dependency/core-only
check and both native jobs at code checkpoint `c7fc9e8d` in
[CI 37803124306](https://github.com/koraytaylan/fsm/actions/runs/37803124306).
Exact job/step inventory digest: `bf272707fbb4e3155d69a9e1b7b808a551cc271cd2f1b54c4269820f08a48a41`;
independently verified native report/log digest: `1b529a1c57073c760e8886b8ee957ddff6b558175fa166841a4b77aa4c74d94d`.
Both compilers cover 82 native cases, 34 production workflow scenarios and
historical original-executor upgrade/drain; the successor through `fd5affe2`
only corrects capability documentation. The 14-file requirement review matches
the frozen worktree, but final task acceptance remains pending, so 9403 stays
in progress. The earlier stable successor stall remains preserved with its
cause unproven; passing successors do not establish a behavioral fix. This
documentation-only verdict omits heavy gates; volatile evidence stays outside
the repository, and the complete 9404 crash inventory remains outstanding.

Frozen 26c68220 passes all six portable gates and both independently verified 82-case native matrices, plus twelve production workflow scenarios per toolchain, including competing live-tree exclusion and final drain; the frozen review has task-cache digest `24b207c7e2fd741f7d298a80c329778fad60d3c374787e5633a6300a18b16cc4`. This establishes the production ownership path used by 8901, not completion of 9401's launch/settlement crash matrix or 9402's full shutdown inventory. Those remaining lifecycle requirements continue to gate final transport integration.

Frozen 89cf8f0c..e501d571 passes genuine native acknowledged-event recovery through a reconstructed paired driver with removed and changed handler tables on stable/MSRV, plus focused lifecycle tests, all-target executor Clippy, formatting and size checks; disabling handoff adoption fails the new recovery assertion and restoration passes. The frozen review has task-cache digest `f11fef60205f63f2791ac541c68926c543b94b660515f4769b86c2b1e7ff3d84`, including the corrected uncertainty-observer race and retained failed evidence. This proves original-event delivery once without another allocation after writer reopen, not executor-kill cutpoints or the full host crash matrix; 9401 remains in progress.

Frozen e3147b2d..0417f9e6 passes the genuine standalone-kill-after-launch path and warm completion reconciliation after another host delivers the original event on stable/MSRV, with focused tests, all-target CLI/executor Clippy, formatting and size checks; neutralizing accepted-event retirement fails the named native assertion and restoration passes. Review digest: `478b2ceee9925d014b51c7295c3ed56b95685fb4a79f0c4465515eaccb38b8ce`. The broader standalone/embedded workflow still fails when native execution is refused as authority busy after a claim, and full CI and the remaining crash cutpoints are unverified; 9401 remains in progress.

Local broker milestones before 7b978a42 served through the retained installed backend; their source pin establishes client and directly invoked helper assertions, but cannot establish changed backend behavior. Fixtures now stage the currently executing backend bytes under the protected authority, while preserving the installed helper's client and gate roles; backend acceptance must use that corrected boundary.

Frozen a243de84..3da1a31a passes all thirteen production workflow scenarios and both enrolled-authorization and provisioned-broker cases on stable/MSRV with the frozen backend, plus focused tests, all-target CLI/executor Clippy, formatting and size checks; review digest `184976e75460ff323fcaebf9546555d1e9ae42f7822c30678e05b6a926eb1e94`. This resolves the earlier local stale-backend contention evidence and corrects warm completion delivery without loosening cold operator routing; full CI and the remaining lifecycle cutpoints are unverified.

Frozen 3da1a31a..5a826175 passes genuine standalone death after verified native closure while the writer remains held, then original timeout settlement and sequential attempt-two recovery on stable/MSRV; review digest `ab97304614bfc85675d4dfa60aca5348dd7bff6b7095cf064bd1078e951e9c71`. The new cut exposed missing native attempted settlements in watcher retry observations; removing that observation reproduces the named stall and restoration passes. This proves the new fourteenth scenario through focused checks, not the full expanded inventory or the remaining claim/stopped/settlement host cutpoints; 9401 remains in progress.

The retained helper is root-owned in the host namespace with its expected digest and 0711 permissions; the restricted sandbox maps host root to nobody. No ownership repair is required, and failed-run artifacts remain preserved.

Frozen 8d5b705e..ad0ac436 passes actual SIGKILL of an independent public Pipeline caller after its durable stopped append while holding the writer, then exact stopped-state recovery, claim exclusion until single-consumption settlement and original event delivery without another allocation on stable/MSRV; review digest `85117ec9330d163ab8c0e8a3f674a38c60a931c3205e15c81a4d064e44b1dde8`. Lifecycle tests, all-target executor Clippy, the genuine enrolled native case, formatting and size checks pass. This proves the public Pipeline cut, not every standalone/embedded/public-tick host at every boundary; remaining claim and post-settlement process cuts and full CI keep 9401 in progress.

Frozen b14c5d51..7d2954e2 passes actual SIGKILL of an independent public Pipeline caller after durable acknowledgement and before its original outcome event, then reconstructed configured-operator delivery once without another allocation on stable/MSRV; review digest `453f0b76fb31138cdd6bad8c75f1315db98a2e941f96910c02b68e82935e9797`. Reopened journal assertions, focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This proves the public Pipeline acknowledgement cut; after-claim and post-attempt process cuts, the remaining host matrix and full CI are unverified, so 9401 remains in progress.

Frozen db5f6273..015efced passes actual SIGKILL of an independent public Pipeline caller after durable retry disposition, then exact nonmutating attempt replay and original backoff recovery on stable/MSRV; review digest `e1c7c19fb86aa1a1436fb00006c5d394d3bdf03934a4ff5c5d4fedc79473a58d`. The successor claim refuses at 1010, accepts as attempt two at 1011 and refuses changed contracts; focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. The retained initial failure was a fixture assertion reading response-envelope fields from the journal body. This proves successor claim timing, not successor launch or the remaining host matrix; after-claim process cuts and full CI remain unverified, and 9401 stays in progress.

Frozen b0e6712d..23ab4d96 extends post-attempt public-host recovery through genuine successor enrollment and final exhausted-timeout settlement on stable/MSRV; review digest `13b60c903e38c86b86088069d9d1e0f7e7da66f5dd40a6cbdf9ff209395bb63f`. The reopened attempt-two claim precedes entry, independent root/descendant membership matches allocation two while the original unit is absent, and stale original completion refuses without mutation. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass; fixture cleanup now accounts for already-retired DynamicUser IPC markers after verified closure. This closes the prior claim-only successor limit, not after-claim process cuts or the remaining host matrix; full CI is unverified and 9401 remains in progress.

Frozen 5dd4aab8..86a32177 passes actual SIGKILL of an independent fresh-claim caller before binding or handler entry on stable/MSRV; review digest `69e998248970fe5b7d2141182847a5412d720411cc488640b2b9fd22c389c834`. The reopened original claim excludes a competitor, genuine unlaunched native closure permits durable interruption, and the pending effect remains unacknowledged with no launch or entry records. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This establishes the public claim-caller boundary, not every standalone/embedded/public-tick host cut or interrupted-successor launch; the remaining host matrix and full CI keep 9401 in progress.

Frozen 7d337545..50597d61 extends fresh-claim crash recovery through a genuine successful successor on stable/MSRV; review digest `a374736b318ce1a2d6a031fedd586d135b296600f6d2d29e5afd9f14c776ff00`. Original interruption replay remains duplicate and nonmutating before and after successor ownership, the run ID advances while attempt one is preserved, and verified successor completion settles once and clears the pending effect. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass; the initial fixture type error was corrected before native execution. This closes the earlier interrupted-successor omission for the public claim-caller path, not the remaining standalone/embedded/public-tick host matrix or full CI; 9401 remains in progress.

Frozen 5a108980..a5e91a11 passes actual production embedded `serve --execute` death during a live native handler and standalone replacement recovery on stable/MSRV; review digest `bd15bd14f7c5bd3a79c7fc10b78f84495e494dcffdfbf982fa4b6f7c99674be4`. Independent PID birth tokens, matched marker identity and genuine closure prove no overlapping tree while the replacement waits behind the held writer; the recovered workflow finishes with valid native claim/stop/settlement history and confirmed drain. Focused CLI/ownership tests, all-target CLI/executor Clippy, eight producer tests, formatting and size checks pass. The fixture now respects the embedded session writer lifetime, and CI includes the fifteenth workflow scenario; only the new case was executed natively here, so the full expanded inventory, remaining host cutpoints and full CI keep 9401 in progress.

Frozen 3b7bba70..17fcb040 passes actual embedded-host death after independently verified native timeout closure, then original settlement and attempt-two recovery, plus all sixteen genuine workflow scenarios on stable/MSRV; review digest `e03851c2f27c6025fab19133cb9f735e4287cf8a6d87aaa9c2a107212ce408d5`. The original server is observed paused with its writer still held, the journal remains at its claim through authenticated tree closure, and SIGKILL precedes replacement recovery. Focused CLI/ownership tests, all-target CLI/executor Clippy, eight producer tests, formatting and size checks pass. This verifies the expanded workflow inventory, not every remaining per-host claim/stopped/ack/event cutpoint or the full native/portable CI matrix; 9401 remains in progress.

Frozen 1f7e011b..5a130b42 passes genuine original-event recovery through direct `service::tick` with removed handlers and borrowed `tick_with` with changed handlers on stable/MSRV; review digest `28547f658efbbf7d7e5d0c3483ad4150345823b4e07399166c81ed49fb560421`. Read-only ticks preserve the journal and outstanding handoff, healthy ticks deliver only the original event once without another allocation, repeated ticks and retained warm completion append nothing, and original components drain with helpers retired and writer released. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This verifies direct public-helper recovery, not every helper process-kill boundary or the remaining host matrix; full CI keeps final acceptance unverified and 9401 remains in progress.

Frozen 5008de0b..c9c2ef02 passes cold public tick recovery after actual stopped-host SIGKILL with removed or changed current handlers on stable/MSRV; review digest `b6c352b5491c26d8bb094b57ac0a587f150cc98f370b96460d5424cab7bb63aa`. Fresh configured-operator components recover protected original completion, append one settlement and the original event without another allocation or stopped record, preserve read-only state, replay nonmutatingly and drain with helpers retired and writer released. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. This closes the retained-completion gap for public stopped-result recovery; remaining host acceptance and the full native/portable CI matrix keep 9401 in progress.

Frozen f9172e3e..7b7b3992 passes genuine cold stopped-run public recovery behind a different physical writer on stable/MSRV; review digest `f6de20dd86fa107be0918ee3c28bc7e7998f47e1dc522785cb8461b9b2492e50`. Public ticks reach writer-unavailable without changing the original journal, claim or stopped ledger, then settle and deliver once after release without another allocation. Independently retained original completion subsequently returns AlreadySettled without append. Focused lifecycle tests, all-target executor Clippy, native enrolled authorization, formatting and size checks pass; remaining host acceptance and the full native/portable CI matrix keep 9401 in progress.

Frozen 45c3beee..7ff302c5 passes genuine stopped-run recovery through two separate configured-operator processes on stable/MSRV; review digest `a291aa4610b87509112f6b1baf16dc5d257f80d3463d8eb01518a933e057a966`. Each reconstructs direct and borrowed public tick drivers with the original executable handler table competing against removed or changed configuration; both reach readiness behind the held physical writer, then consume one original settlement/event without another allocation, replay nonmutatingly and confirm both helper inventories drained. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass. Review corrected a release-observer race and expected borrowed-writer contention in the fixture, preserving the initial failed evidence. This replaces the earlier interleaved-only verdict, not the exhaustive 9404 matrix; full native/portable CI remains unverified and 9401 stays in progress.

Frozen 3c295bf6..6216ae49 passes all six genuine broker client-death process/MCP routes on stable/MSRV; review digest `d1904061e9e3661521eeb58a5042598beb6febb67700fed883a116073bf8ebe2`. Both native CI axes exposed the fixture's obsolete single-thread expectation after adopting the frozen test-binary backend: closure and tree retirement had succeeded, but the test harness retains its main and test threads. Acceptance now requires live workers to appear and their exact retirement to the original idle thread identities, preserving independent closure, domain removal and the original deadline. Focused lifecycle tests, all-target executor Clippy, formatting and size checks pass; failed CI and local evidence remain retained, full CI is unverified, and 9401 remains in progress.

Frozen 26c68220..2b580fd7 completes 9401 against its unchanged ownership inventory: all six stable/MSRV portable gates and both independently verified 82-case native matrices pass, with sixteen original production workflow scenarios per compiler and the explicit dependency/core-only graph check. Independent real-process crash and competition fixtures cover claim, launch, verified stop, stopped publication, attempt/ack and original-event recovery, including public direct/borrowed ticks, changed/removed handler tables, writer contention and stale completion. Review digest: `1d12bef64d598e422de8caaa09f667e2da4e34ff9a0d5f4cf9005f596438f584`; CI: [37713754061](https://github.com/koraytaylan/fsm/actions/runs/37713754061). Native capability remains approved Linux/systemd only; the full 9402 shutdown inventory, 9403 reconciliation and exhaustive 9404 matrix remain incomplete.

Frozen 6e02a48c..9e807cea passes genuine standalone and embedded ENOSPC shutdown on stable/MSRV: a two-MiB noswap store fills after handler-tree entry, production abort returns bounded uncertainty, original endpoint observations prove admission closure, and the unchanged durable claim survives until authenticated native closure and public-API interruption reconciliation permit sequential restart. Focused compilation, all-target CLI/executor Clippy, formatting and size checks pass; review digest `76e7c27eff59e8b3f78885615dd80bd27aaa556b02813eb7c35d76d6545e543a`. This establishes the full-disk shutdown path, not automatic closure-only orphan recovery or production CLI reconciliation, which remain 9403 requirements; failed-native-stop acceptance, complete 9402 criteria and the full integration gate remain open.

Frozen fcef3627..df91c38f passes genuine standalone and embedded failed-native-stop shutdown and authenticated repair on stable/MSRV: withholding the original protected handoff produces bounded production abort uncertainty and an independent broker refusal, while admission closes and the unchanged journal retains its original claim without fabricated events. Restoring the same handoff inode permits public-API stopped/interrupted settlement and sequential successful workflow recovery; all four fixtures retire cleanly. Focused compilation, all-target CLI/executor Clippy, formatting and size checks pass; review digest `8b35ad951f4ea15f7381b9a697cbb44ce2ca5fb462403e370ef21c6787092dba`. Complete 9402 acceptance and the full integration gate remain unverified; automatic closure-only recovery and production CLI reconciliation remain 9403 requirements.

9402's preliminary unchanged-inventory audit identifies remaining dedicated production proof for healthy drain/abort of already-running trees, completing drain, quiet embedded admitted-handler stop and timeout without new work; empty quiet shutdown and final workflow drain cannot substitute for those cases. Audit digest: `5f3b8e007a211d7826fd9ccae6c324b9fa7466f86681e60558d2270577924da5`. Integration remains unverified: Ubuntu CI exposed a final-diagnostic fixture deadline expiring before actual backpressure; 470687bd preserves its blocked-worker and bounded-exit assertions with rendering time included, and focused stable/MSRV tests, all-target CLI Clippy, formatting and size checks pass.

Frozen 3f6ecc60..6577792d passes eight genuine stable/MSRV standalone/embedded live-tree cases: healthy production abort and observed drain-to-abort escalation return stopped with complete inventory, retired helpers and released writer, preserve the original pending instance through authenticated interruption records, and permit sequential successful recovery. Embedded stdin remains open without additional frames or EOF; focused compilation, all-target CLI/executor Clippy, formatting and size checks pass. Review digest: `f51dfe540a8b686ba381040d64eef0b0300562c722f96b9095b40d9cced7ce62`. This closes the healthy abort and quiet active-handler stop gaps, while natural-completion drain, drain expiry, quiet admitted timeout without new work and full integration remain unverified; the two initial fixture failures remain preserved.

Frozen 127d60aa..61cdf2c9 passes eight genuine stable/MSRV standalone/embedded natural-completion and quiet admitted-timeout drain cases: admission closes while the original tree is live, successful drain preserves its acknowledged original event, and unreleased trees instead reach their original handler timeout without new entry or machine events before sequential restart. Embedded stdin stays open without further frames or EOF; focused compilation, all-target CLI/executor Clippy, formatting and size checks pass. Review digest: `883565a8d261d2575d4b0a1a1bf2b6ed3777ad328e63462b26175cce30ba0ad1`. Quiet timeout is proved during explicit drain with admission closed; automatic plan 0020 scheduling is not claimed to start no new work. Drain-expiry verification, full acceptance audit and integration remain open; the new expiry fixture now waits for original authenticated publication before recovery after its initial embedded refusal.

Frozen 61cdf2c9..a23be382 passes four genuine stable/MSRV standalone/embedded one-second drain-expiry cases against thirty-second live trees: production control returns bounded uncertainty, the original owner exits ordinarily, authenticated closure precedes public-API interrupted settlement, and pending state survives without fabricated events before sequential recovery; review digest `1887db669d25e4571f50b9b4e31aa504fd924c4f6b5b36dcd11d003350865534`. Frozen da2243b8..1a49d8e9 additionally proves actual handler stdin EOF through quiet embedded abort on stable/MSRV, with the MCP input still open; review digest `e30b54bc6df2913793a83bcd140613fee3037a3702384a9b035c7231641b2b1e`. Focused compilation, all-target CLI/executor Clippy, formatting and size checks pass; the initial embedded expiry failure remains retained. These close the scoped expiry and protocol-input gaps, while complete 9402 requirement audit and full integration remain unverified; automatic closure-only and production CLI reconciliation remain 9403 requirements.

Frozen 2b580fd7..5730f172 completes 9402 against its unchanged shutdown inventory: all nine [CI jobs](https://github.com/koraytaylan/fsm/actions/runs/37727458276) pass, with six independently inspected portable debug/release gates and both verified 82-case native matrices plus thirty-four production workflow scenarios per compiler. Public controls and genuine standalone/embedded fixtures prove finite drain/abort, immediate admission closure, quiet admitted-handler progress, original-deadline preservation, authenticated interruption, actual signal exits, writer contention, ENOSPC, failed native stop and protocol-input isolation; final review digest `4e1c4842132ace74a0a960b3bde56155d1f5d5dca74a331c800c04d8182fba0b`. Native capability remains Linux/systemd only, native signals use the approved termination/lease-EOF policy, and Drop remains outside the guarantee; artifact executable bytes were not independently compared. Quiet timeout without new work is proved during explicit drain, not automatic scheduler inactivity. Automatic closure-only recovery and production operator reconciliation remain 9403 requirements, and the exhaustive 9404 crash matrix remains incomplete; this documentation-only completion update omits heavy gates.

Frozen 5367470e..86e74c26 verifies production standalone and embedded CLI orphan repair on stable/MSRV: both independently checked native matrices pass 82 cases and 34 workflow scenarios, including retained failed-stop claims, authenticated interrupted settlement and sequential restart; review digest `cb2cc20c62426b9eaab5239e494d180c0fccf12e67d3d8d1ed34f7370aeb134b`. Frozen 86e74c26..0d9f63b9 additionally passes focused genuine stable/MSRV original-result service recovery with a changed handler table, preserving the exact prefix and original successful outcome before startup delivers its event once without another launch; review digest `0bf11e442ff00b716bed572c410971c82572743a09ac4eff2014673a3d016cd1`.

Frozen 86e74c26..6d08943f reviews original-result recovery and guarded startup closure; review digest `604fe6bfed61c9174f640f4e8a734d9f20f3bd6a0e15320d490b7939ae67c6f1`, including completion-material guard pass/fail/pass sensitivity.

Frozen 6d08943f..e80bacf4 passes focused genuine original-result duplicate replay and stale-run replay alongside a live native successor on stable/MSRV; review digest `0269967e1ab97f5d1938799db87cba64badd87fd3b8c863afa26a3cbf3f867a3`. The prior [e0853c06 CI checkpoint](https://github.com/koraytaylan/fsm/actions/runs/37737196667) now passes all nine jobs, and its [snapshot](https://github.com/koraytaylan/fsm/actions/runs/37737196699) completes all six portable gates and artifact publication; both native axes independently verify 82 cases and 34 workflow scenarios. Current startup orphan/live-runner and repeated CLI/concurrent-writer fixtures compile, but native execution and current full integration remain pending; pre-run ownership and dedicated identity/reset acceptance remain incomplete, so 9403 stays in progress. Volatile evidence remains in the task cache; this documentation-only status update omits heavy gates.

Frozen 9414bb43..c3d01895 passes genuine copied-store original-result refusal and original-store recovery on stable/MSRV, preserving both journal prefixes and the installed helper; focused review digest `fb18f9170b47744fb0cdd5736ef076cddfd1b0c9d7bc890e71f8f7d93c091106`. Seven stable production CLI inspection/reconciliation tests pass, including bounded invalid-deadline refusal without store initialization. The range also corrects public polling in the live-runner fixture and bounded EOF observation in private exec-status fixtures; the latter passes four focused tests on both compilers. Corrected startup native execution, current full integration, pre-run ownership, identity/reset acceptance and executed operator transcripts remain unverified, so 9403 remains in progress; current focused CLI lint is pending. This documentation-only milestone update omits heavy gates, and volatile evidence stays in the task cache.

Frozen bd04ea56..a0fd6a14 wires protected pre-run owner leases, owned preparation collection and continuous transfer into the original execution owner, and authenticates absent original bindings while holding both original leases; preliminary review digest `a6f158314faf48553b6e8ab3ac841e64b1f18635d969e20da860a52e91b24dc3`. Stable executor all-target Clippy, formatting, source-size and frozen-range diff checks pass; focused unit execution is pending. Review corrected late guard delivery and the legacy subprocess fixture contract. Both prior native workflow axes reached the failed-stop scenario and exposed its obsolete generic refusal assertion, corrected here without changing journal/tree recovery checks. Current native guard sensitivity, absent-binding operator/startup acceptance, legacy public API ownership audit, identity/reset/race cases, executed transcripts and full integration remain outstanding, so 9403 stays in progress; this checkpoint grants no completed pre-run recovery claim. Volatile evidence stays in the task cache.

Frozen 6a0dc022..33c84c61 corrects client `prepare-owned` routing and requires the original owned guard on default public preparation, rejecting legacy operator allocation requests while retaining controlled legacy recovery fixtures; review digest `0a473473ff92d5a72d2762de55c9e366140b002f683c3fa1177bf49cd488d347`. The unreleased Rust `poll` return type and migration are documented. Three focused client-policy tests pass, and the public startup legacy refusal passes original/neutralized/restored sensitivity at exits 0/101/0. The unchanged owner/runner lease modules pass the genuine root kernel fixture without changing the retained helper; stable all-target executor Clippy passed at e2724c68. The new pre-binding operator/startup recovery fixture compiles on stable but remains natively unexecuted; current broker regression execution, corrected-API MSRV, full integration, identity/reset/race evidence and operator transcripts remain outstanding, so 9403 stays in progress and this range grants no end-to-end recovery completion. Volatile evidence stays in the task cache.

Frozen 33c84c61..d705fc4f passes all six portable full gates and both provisioned native matrices and CLI workflow inventories on stable/MSRV at the exact clean source in [CI](https://github.com/koraytaylan/fsm/actions/runs/37759134797); review digest `263b95e77b624b491f7ad7aa8a93c79d71704ebf582abe870efeb2b5c4b1de8b`. Full integration review digest: `3b3b96502bc79ec5fcf7d73bcf8b9b8396a032f6934f55892f6be1a84bee2bd4`. Authenticated reports and logs cover live preparation-owner refusal, absent-binding startup interruption without handler entry, exact duplicate replay and continuous owner transfer through retirement. Review corrected the omitted public API inventory and duplicate fixture permission grant while preserving the strict ownership assertion. Broker legacy refusal passes original/neutralized/restored sensitivity at 0/101/0 with exact source restoration. Native owner-guard sensitivity, dedicated identity/reset/race acceptance audit, executed operator/upgrade transcripts and successor integration remain outstanding, so 9403 remains in progress; this documentation-only milestone omits heavy gates, and volatile evidence stays in the task cache.

Frozen d705fc4f..ab66d203 passes both focused provisioned native matrices and all CLI workflow groups on stable/MSRV in [CI](https://github.com/koraytaylan/fsm/actions/runs/37767981476); review digest `25295a0e6abe85ab9945dd1ead5dae483b7ae1829c7b5bcfa1fa1b6ac419ab1c`. Exact clean-source reports and matched log hashes prove pre-binding copied-store, changed lease-protection and wrong-boot refusals, plus six retained operator interruption/replay transcripts per compiler and complete fixture/helper retirement. Review repaired transient authority contention in the hidden-handoff fixture and a transcript-case variable shadowing the installer digest without weakening either refusal or identity checks. Eight focused stable CLI tests, three MSRV MCP tests and focused stable CLI Clippy pass, covering nonmutating verified-prefix inspection and unsupported-backend refusal. The focused dispatch intentionally omits portable full gates; the independent-owner sensitivity fixture, complete production identity/reset/race audit, executed upgrade acceptance and current full integration remain unverified, so 9403 stays in progress. Volatile evidence remains in the task cache; this documentation-only milestone omits heavy gates.

Frozen ab66d203..72ba52f5 proves public pre-binding preparation-owner exclusion sensitivity at original/neutralized/restored exits 0/101/0 on stable/MSRV in [CI](https://github.com/koraytaylan/fsm/actions/runs/37769348623); review digest `08eceb7e508e5ebc5c4acda035914a4ff2a0555fb1298b4274f45db3344ec4e4`. The fixture holds the original kernel lock independently of production acquisition; removing only that guard makes the named public refusal test fail on unintended interrupted settlement, and exact source restoration re-passes. Authenticated phase logs retain the deliberately failed isolated authority and unchanged installed helper. Eight focused reconciliation tests and CLI fixture Clippy now pass on both compilers, alongside the three MCP tests. The focused proof does not replace full portable integration, and complete production identity/reset/race and upgrade acceptance remain outstanding, so 9403 stays in progress. Volatile evidence remains in the task cache; this documentation-only milestone omits heavy gates.

Frozen e8565850..afcf253c passes public CLI reconciliation refusal before original handoff repair, authenticated original settlement and duplicate replay in all 29 native CLI workflow groups on stable/MSRV in [CI](https://github.com/koraytaylan/fsm/actions/runs/37771386906); review digest `ee39cc0c42409890611bd1c36242fc9d50da67025dcfa9d4e9fcce81cf5719ad`. The unrepaired command retains the exact journal claim, returns bounded `exec/inflight_deferred` with the recovery hint and launches no additional handler; both fixtures retire after original repair and sequential restart. Focused workflow and lifecycle Clippy pass, but subsequent contention and live MCP orphan fixture changes await native execution, and upgrade acceptance, the remaining identity/race inventory and current full integration remain open; 9403 stays in progress. This focused milestone omits portable full gates, and volatile evidence stays in the task cache.

Frozen afcf253c..bd639974 passes concurrent first closure of live orphan process and MCP trees, sequential restart, concurrent stale-run isolation, production CLI owner/identity refusals and historical executor inspection/drain on stable/MSRV in [CI](https://github.com/koraytaylan/fsm/actions/runs/37788184355); review digest `e9163ac22e5828d8f01b6d6f59cc28c130f96d030bbfd3a4bb218e05058aea49`. Two production callers yield exactly one first closure and only duplicate success or the exact writer-lock refusal for the competitor; exactly two durable records precede successor enrollment, and stale callers preserve the journal and independently observed successor membership. Copied-store, wrong-boot, changed lease protection, live preparation/execution-owner and inaccessible original-socket cases retain ownership with bounded typed hints and no additional launch; restoring the same protected socket permits legitimate original recovery. All native suites and 29 ordinary CLI workflow groups (34 scenarios) pass with authenticated report/log hashes. Both historical standalone/embedded fixtures preserve the exact CLI, broker and helper from 5730f172, verify six retained/stop/drained transcripts per compiler and retire matched fixtures cleanly. Focused lifecycle Clippy passes on stable/MSRV. This grants no authority replacement, removed-facility/environment-reset or production PID/domain-reuse proof; those identity acceptance gaps and current full portable integration keep 9403 in progress. This documentation-only milestone omits heavy gates, and volatile evidence stays in the task cache.

Frozen bd639974..f4ffa7c0 additionally verifies recorded-domain identity mismatch refusal through both public API and production CLI for original process/MCP trees, preserving live original membership before exact protected-file restoration and legitimate closure; native review digest `a458ef9cf96f5d08430cec7eeafe30ad5fa49521caba77578e3a91a515298a93`. The earlier bd639974 integration checkpoint passes all six portable gates and both native jobs in [CI](https://github.com/koraytaylan/fsm/actions/runs/37786876783); authenticated job/step inventory digest `96865124029ebaea0c2280fb710e8a8ed06e2b68f0f99ff2abb087bdfb24636a` applies only to that source. Successor range f4ffa7c0..1b6df856 fixes competing CLI timeout cleanup and adds physical cgroup-path substitution and absent recorded-endpoint fixtures. Native execution at 22f6b5d8 failed on both compilers because cgroup v2 rejects rename, before reuse acceptance; 1b6df856 preserves the original allocation beneath a fixture-owned bind mount instead. Independently checked failure logs and preliminary range review have digest `7ec7da6d9509371eacee1ad096d2f49c6ea602c53368570da00637b6c9e408ba`. At 1b6df856 both 82-case native containment inventories pass, including empty physical-path substitution and missing-endpoint refusal; stable workflow/upgrade pass, but the MSRV ordinary workflow fails with pre-entry authority contention, so the complete checkpoint fails (review digest `3eb0048aae39ab7708889c6cac5a152e8dbbea6d96e7b18beee578ea05a13633`). Successor 314ab9f2 bounds association lock acquisition within its original two-second deadline and adds release/exhaustion cases plus a live replacement-domain sentinel. Focused stable all-target executor Clippy, formatting, size and diff checks pass; successor native acceptance, association guard sensitivity, actual PID reuse and successor full portable integration remain unverified, so 9403 stays in progress. This documentation-only checkpoint omits heavy gates, and volatile evidence stays in the task cache.

Historical STATUS evidence is retained outside the repository in the task-cache
plan-status-archives directory, addressed by SHA-256:
`cdcc311f66d08ae7b640731ed4a159d2aaf25863b59dd833afca7a99b3e9a9c9`.
Volatile sessions, PIDs and intermediate logs stay in the task cache and do not establish completion.
Frozen 1b6df856..314ab9f2 passes both 82-case native containment inventories,
all 29 ordinary CLI workflow groups and historical executor upgrade inspection
and drain on stable/MSRV in [CI](https://github.com/koraytaylan/fsm/actions/runs/37796660305);
verified report/log/transcript digest
`f32a2916a7ec011ad24d3c87e1ab48eee41b2e4a6239c6d5edf0a54e26450549`.
This proves the live replacement-domain sentinel survives identity refusal,
missing recorded-endpoint refusal and bounded association contention at that
source; the prior MSRV workflow contention failure is retained as a failed
checkpoint. Successor held-lock assertion and guard-neutralization evidence,
actual PID reuse, current portable integration and the complete task acceptance
audit remain outstanding, so 9403 stays in progress. This documentation-only
milestone omits heavy gates; volatile evidence remains in the task cache.

Frozen 314ab9f2..04bc21e2 verifies actual kernel PID reuse for original process
and MCP trees on stable/MSRV: unrelated reused-PID sentinels survive public
identity refusal and legitimate original-domain closure; scoped review digests
`8c896e1275bc8bb452061425ba1194b4d4409a806a2fd3867b84268526270b8a`
and `7ff50864f01269f042f5e0aee4f76a1f1a554f956fa8c7a5654906d4c7e7eddf`.
Association-deadline sensitivity passes original/neutralized/restored at
0/101/0 on both compilers (review digest
`6d9594dde20ec36a4de170bc32e360ebcb28baf5818ab1bd9f126feaf22ac1ef`).
The checkpoint fails: MSRV completes all workflow and historical upgrade
cases, but stable stalls during embedded-abort successor recovery with an
unresolved claim; the cause remains unproven. Successor a22a1b1e adds bounded
actual-owner diagnostics without weakening recovery assertions. Current full
integration and final acceptance review remain unverified, so 9403 stays in
progress; this documentation-only verdict omits heavy gates.

Frozen `422a5f0f..e9653716` independently verifies the shared physical cgroup
identity guard through all six callers: binding, exec-status listener,
selected-group grant, launch, runner and genuinely enrolled grant, plus public
completion original journal-claim closure matching on Linux stable/MSRV;
consolidated scoped verdict digest
`3d475af0f2689f134be0a404142455cce3156dbfd60a9684cab2d95fe8b46baa`.
Each named case passes, fails at exit 101 after exactly the selected guard is
neutralized, then passes after restoration; independent later enrollment
refusals remain enabled and cannot excuse prior submission or private-status
publication. The original `b91d905e` launch/runner checkpoint failed because
its fixture republished an existing immutable binding; corrected `33862c07`
verifies restoration through genuine execution and matched closure, with
unchanged binding and journal ownership. Failed fixtures retain their native
authorities without environment-reset clearance. Remaining closure caller
sensitivities, termination/crash axes and current full integration keep 9404
in progress; detailed evidence stays in the task cache, and this docs update
omits heavy gates.

Frozen `9f15c883..83998a77` review independently verifies execution completion
transfer runtime sensitivity on Linux stable/MSRV against exact source and
log digests; scoped verdict digest
`a084705b028d4dca80f10959f6756f233f55e5d7681243650db5ff7468d1236f`.
The frozen reports incorrectly name an unrelated public refusal despite the
correct named test passing, failing at exit 101 with only its guard removed,
and passing after restoration; `83998a77` corrects that metadata, pending
successor CI rather than retroactively relabeling historical evidence.
Integration checkpoint `d8125e9e` completed both native runtime suites but
failed artifact retention because nested Rust test names introduced colons in
log filenames; `f3dbedc9` uses portable filenames, preserves exact report test
identities and adds frozen native evidence verification to CI, with focused
Python checks passing and native successor evidence outstanding.
Neither repair establishes full integration or task completion; 9404 remains
in progress, detailed evidence stays in the task cache, and this documentation
update omits heavy gates.
