"""The acceptance scenarios, one function each.

Every scenario that used to read `manual:` in `docs/RELEASE.md` is here, named
after the item it replaces. Two rules hold throughout:

* **Drive the shipped artifacts.** The `fsm` on `PATH` is the one
  `cargo install --locked` produced; the MCP client is a separate
  implementation that shares nothing with the engine. A suite built out of the
  engine's own helpers would agree with it by construction.
* **Assert what a person was being asked to look at.** The manual list did not
  say "run a migration"; it said confirm the grouped refusal summary reads
  correctly — the counts, the codes, and the state responsible. So the
  assertions are about those, not about an exit code.
"""

from __future__ import annotations

import json
import os

from . import fsm
from .mcp import HttpClient, McpError, StdioClient

CASE_REVIEW = "case_review.json"
EXPECTED_TOOLS = 24
TOOLS_LIST_CEILING = 38_000


def _spec(path: str) -> dict:
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def _serve(store: str, *extra: str) -> StdioClient:
    return StdioClient([fsm.FSM, "serve", f"--data-dir={store}", *extra])


# --------------------------------------------------------------------------
# manual: <host>: connect, list all 24 tools, run the golden loop end-to-end.
# --------------------------------------------------------------------------

def tools_list_is_complete_and_within_its_budget(report) -> None:
    """What "connect and list all 24 tools" was actually checking.

    The count is the easy half. The other half is that `tools/list` fits: it
    measures ~36 KiB against a 38 000-byte ceiling, and a host that truncates
    or refuses an oversized payload is exactly what a human clicking through
    Claude Desktop would have noticed.
    """
    with fsm.Scratch("tools") as scratch, _serve(scratch.dir("store")) as client:
        info = client.initialize()
        report.note(f"serverInfo {info['serverInfo']['name']} {info['serverInfo']['version']}")
        report.equal(
            info["serverInfo"]["version"],
            fsm.run("version").ok().out.strip().split()[-1],
            "serverInfo.version matches the binary's own version",
        )
        tools = client.tools()
        report.equal(len(tools), EXPECTED_TOOLS, f"tools/list offers {EXPECTED_TOOLS} tools")
        measured = len(json.dumps({"tools": tools}, separators=(",", ":")))
        report.note(f"tools/list measures {measured} bytes")
        report.true(
            measured < TOOLS_LIST_CEILING,
            f"tools/list fits its {TOOLS_LIST_CEILING}-byte budget (measured {measured})",
        )
        for tool in tools:
            report.true(
                bool(tool.get("description")) and "inputSchema" in tool,
                f"{tool['name']} carries a description and an input schema",
            )


def the_golden_loop_runs_end_to_end(report) -> None:
    """The loop every host item asked for, written down at last.

    `docs/RELEASE.md` named "the golden loop" three times and defined it
    nowhere, so each release it meant whatever the person running it
    remembered. It is this: define a machine, create an instance, advance it,
    acknowledge the effect that advance emitted, drive it to a terminal state,
    and read the history back.
    """
    with fsm.Scratch("golden") as scratch, _serve(scratch.dir("store")) as client:
        client.initialize()
        created = client.structured(
            "machine_create", {"spec": _spec(fsm.fixture_machine(CASE_REVIEW))}
        )
        report.true(created.get("created") is True, "machine_create created the machine")
        machine = created["machine_id"]

        instance = client.structured(
            "instance_create", {"machine": machine, "request_id": "golden-create"}
        )
        instance_id = instance["instance_id"]
        report.equal(instance["leaf"], "intake", "a new instance starts in intake")

        advanced = client.structured(
            "instance_send",
            {
                "instance_id": instance_id,
                "event": {"name": "docs_ok"},
                "request_id": "golden-1",
            },
        )
        report.equal(advanced["leaf"], "docs_review", "docs_ok enters the review compound")
        pending = advanced.get("effects_pending") or []
        report.equal(len(pending), 1, "entering in_review emitted exactly one effect")

        acked = client.structured(
            "effect_ack",
            {
                "instance_id": instance_id,
                "effect_id": pending[0],
                "outcome": "ok",
                "request_id": "golden-ack",
            },
        )
        report.equal(acked.get("effects_pending") or [], [], "the ack cleared the outbox")

        client.structured(
            "instance_send",
            {
                "instance_id": instance_id,
                "event": {"name": "docs_ok"},
                "request_id": "golden-2",
            },
        )
        settled = client.structured(
            "instance_send",
            {
                "instance_id": instance_id,
                "event": {"name": "scored", "payload": {"score": "900"}},
                "request_id": "golden-3",
            },
        )
        report.equal(settled["leaf"], "approved", "a score above the bar approves")
        report.equal(settled.get("status"), "completed", "approving completes the instance")

        history = client.structured("instance_history", {"instance_id": instance_id})
        kinds = [entry.get("kind") for entry in history.get("entries", [])]
        report.true(
            "InstanceCreated" in kinds and "EventApplied" in kinds,
            f"the history carries the creation and the applied events: {kinds}",
        )
        report.true(
            history.get("chain_verified") is True,
            "the history reports a verified hash chain",
        )

        doctor = client.structured("store_doctor")
        report.equal(doctor.get("health"), "Ok", "store_doctor reports a healthy store")
        verified = client.structured("journal_verify")
        report.equal(verified.get("health"), "Ok", "journal_verify reports a healthy store")


def a_rejected_event_is_refused_rather_than_silently_ignored(report) -> None:
    """The half of a host check that catches a server reporting success wrongly."""
    with fsm.Scratch("reject") as scratch, _serve(scratch.dir("store")) as client:
        client.initialize()
        machine = client.structured(
            "machine_create", {"spec": _spec(fsm.fixture_machine(CASE_REVIEW))}
        )["machine_id"]
        instance = client.structured(
            "instance_create", {"machine": machine, "request_id": "reject-create"}
        )["instance_id"]
        refusal = client.try_call(
            "instance_send",
            {
                "instance_id": instance,
                "event": {"name": "scored", "payload": {"score": "900"}},
                "request_id": "reject-1",
            },
        )
        report.true(
            refusal.get("isError") is True,
            "an event the state does not handle is reported as an error",
        )
        rendered = json.dumps(refusal)
        report.true("run/" in rendered, "the refusal carries a run/* code")


# --------------------------------------------------------------------------
# manual: a real MCP client over the HTTP transport — initialize through
# teardown, and at least one notification on the SSE stream.
# --------------------------------------------------------------------------

def the_http_transport_serves_a_session_and_pushes_a_notification(report) -> None:
    """A client that holds a stream open while the session goes on being used.

    The reason this was manual: "a conformance suite driving a socket is not
    the same as a client that has to like what it sees". What made that true
    was that every test around it drove the endpoint one request at a time, so
    nothing held a stream open across an advance. This does — the GET lives on
    its own connection while a POST on another advances the instance.
    """
    with fsm.Scratch("http") as scratch:
        store = scratch.dir("store")
        port = fsm.free_port()
        with fsm.Serving(store, port), HttpClient("127.0.0.1", port) as client:
            info = client.initialize()
            report.true(bool(client.session), "the server issued an Mcp-Session-Id")
            report.equal(
                info["protocolVersion"], "2025-06-18", "the session negotiated the current version"
            )

            machine = client.structured(
                "machine_create", {"spec": _spec(fsm.fixture_machine(CASE_REVIEW))}
            )
            report.true(machine.get("created") is True, "machine_create over HTTP succeeded")
            created = client.structured(
                "instance_create",
                {"machine": machine["machine_id"], "request_id": "http-create"},
            )
            instance_id = created["instance_id"]

            # Hold the event stream open, then advance the instance from a
            # separate connection and wait for the push.
            client.open_stream()
            client.request(
                "resources/subscribe", {"uri": f"fsm://instance/{instance_id}"}
            )
            client.call(
                "instance_send",
                {
                    "instance_id": instance_id,
                    "event": {"name": "docs_ok"},
                    "request_id": "http-advance",
                },
            )
            event = client.await_event(timeout=30)
            report.note(f"stream delivered {event.get('method')}")
            report.true(
                event.get("method", "").startswith("notifications/"),
                "a notification arrived on the open event stream",
            )

            status = client.delete_session()
            report.true(
                status in (200, 204),
                f"the session tore down cleanly (HTTP {status})",
            )


def the_http_transport_refuses_a_request_without_its_session(report) -> None:
    """Teardown is only real if the session stops working afterwards."""
    with fsm.Scratch("http-teardown") as scratch:
        port = fsm.free_port()
        with fsm.Serving(scratch.dir("store"), port), HttpClient("127.0.0.1", port) as client:
            client.initialize()
            client.delete_session()
            status, _headers, body = client.post(
                {"jsonrpc": "2.0", "id": 99, "method": "tools/list"}
            )
            report.true(
                status >= 400 or "error" in body,
                f"a deleted session no longer serves requests (HTTP {status})",
            )


# --------------------------------------------------------------------------
# manual: drive a parent-and-child workflow through a live MCP host.
# --------------------------------------------------------------------------

def a_parent_and_child_workflow_runs_and_reads_back_as_a_tree(report) -> None:
    with fsm.Scratch("compose") as scratch, _serve(scratch.dir("store")) as client:
        client.initialize()
        child = client.structured(
            "machine_create", {"spec": _spec(fsm.example("case_review_child.json"))}
        )["machine_id"]
        parent = client.structured(
            "machine_create", {"spec": _spec(fsm.example("case_review_parent.json"))}
        )["machine_id"]
        report.note(f"child {child}")

        instance = client.structured(
            "instance_create", {"machine": parent, "request_id": "compose-create"}
        )["instance_id"]
        # The slot lives in `delegating`; a slot appears from the moment its
        # state is entered, so the parent has to get there first.
        client.structured(
            "instance_send",
            {
                "instance_id": instance,
                "event": {"name": "open"},
                "request_id": "compose-open",
            },
        )
        view = client.structured("instance_get", {"instance_id": instance})
        slots = [entry["slot"] for entry in view.get("children", [])]
        report.true(bool(slots), f"the parent declares an invocation slot: {slots}")
        slot = slots[0]

        started = client.structured(
            "invocation_start",
            {"instance_id": instance, "slot": slot, "request_id": "compose-invoke"},
        )
        child_id = started["child_instance_id"]
        report.note(f"slot {slot} enacted child {child_id}")

        tree = client.structured("instance_get", {"instance_id": child_id})
        report.equal(
            (tree.get("parent") or {}).get("instance_id"),
            instance,
            "the child reads back its parent",
        )
        report.equal(
            (tree.get("parent") or {}).get("slot"), slot, "and the slot it was invoked from"
        )

        roots = client.structured("instance_list", {"roots_only": True})
        listed = [row["instance_id"] for row in roots.get("instances", [])]
        report.true(instance in listed, "the parent is listed as a root")
        report.true(child_id not in listed, "the child is not listed as a root")

        by_parent = client.structured("instance_list", {"parent": instance})
        report.equal(
            [row["instance_id"] for row in by_parent.get("instances", [])],
            [child_id],
            "filtering by parent finds exactly the child",
        )


# --------------------------------------------------------------------------
# manual: drive a reactive machine and confirm each cascade is one macrostep.
# --------------------------------------------------------------------------

def a_reactive_cascade_reads_as_one_macrostep(report) -> None:
    """The fork/join example, and the claim that a cascade is one record."""
    with fsm.Scratch("reactive") as scratch, _serve(scratch.dir("store")) as client:
        client.initialize()
        machine = client.structured(
            "machine_create", {"spec": _spec(fsm.example("parallel_fork_join.json"))}
        )["machine_id"]
        instance = client.structured(
            "instance_create", {"machine": machine, "request_id": "reactive-create"}
        )["instance_id"]
        advanced = client.structured(
            "instance_send",
            {
                "instance_id": instance,
                "event": {"name": "approve"},
                "request_id": "reactive-1",
            },
        )
        report.note(
            "approve settled at "
            f"{advanced.get('leaf') or advanced.get('configuration') or advanced.get('leaves')}"
        )

        history = client.structured(
            "instance_history", {"instance_id": instance, "include_trace": True}
        )
        applied = [
            entry
            for entry in history.get("entries", [])
            if entry.get("kind") == "EventApplied"
        ]
        report.equal(len(applied), 1, "the whole cascade is a single applied record")
        microsteps = applied[0].get("microsteps") or []
        report.true(
            len(microsteps) >= 1,
            f"the record carries its reaction microsteps ({len(microsteps)})",
        )
        report.true(
            all("trigger" in step for step in microsteps),
            "every microstep names what triggered it",
        )

        explained = client.structured(
            "explain_step", {"instance_id": instance, "seq": applied[0]["seq"]}
        )
        report.true(
            "microsteps" in json.dumps(explained),
            "explain shows the same cascade for that sequence",
        )


# --------------------------------------------------------------------------
# manual: preview and then migrate a live cohort, and confirm the grouped
# refusal summary reads correctly to a person.
# --------------------------------------------------------------------------

def a_cohort_preview_groups_its_refusals_legibly(report) -> None:
    """The item asked whether a *report* is legible, so that is what is asserted.

    A cohort whose instances sit in more than one state, migrated onto a
    definition whose mapping covers only some of them. What a person was being
    asked to read is here as assertions: a count, a code, and the state
    responsible for each exclusion.
    """
    with fsm.Scratch("cohort") as scratch:
        store = scratch.dir("store")
        fsm.run("machine", "add", fsm.fixture_machine(CASE_REVIEW),
                data_dir=store).ok()
        old = fsm.run_json("machine", "ls", data_dir=store)["machines"][0]["machine_id"]
        digest = old.split("sha256:")[1]

        # Three instances, deliberately in three different states.
        for index, events in enumerate([[], ["docs_ok"], ["docs_ok", "docs_ok"]]):
            instance = f"c{index}"
            fsm.run("instance", "new", "case_review", f"--request-id=c-{index}",
                    data_dir=store).ok()
            for step, event in enumerate(events):
                fsm.run("instance", "send", f"inst-c-{index}", event,
                        f"--request-id=c-{index}-{step}", data_dir=store).ok()
            report.note(f"{instance} prepared with {len(events)} events")

        listed = fsm.run_json("instance", "ls", data_dir=store)["instances"]
        states = sorted({row["state"] for row in listed})
        report.true(len(states) > 1, f"the cohort spans more than one state: {states}")

        # A superseding definition whose mapping covers `intake` only.
        spec = _spec(fsm.fixture_machine(CASE_REVIEW))
        spec["name"] = "case_review_v2"
        spec["supersedes"] = {
            "machine": digest,
            "states": {"intake": "intake"},
            "context": {"visits": "ctx.visits", "notes": "ctx.notes", "score": "ctx.score"},
        }
        new_path = scratch.write("v2.json", json.dumps(spec))
        fsm.run("machine", "add", new_path, data_dir=store).ok()

        previews = [
            fsm.run_json("instance", "migrate", row["instance_id"],
                         "--to=case_review_v2", "--dry-run", data_dir=store)
            if fsm.run("instance", "migrate", row["instance_id"],
                       "--to=case_review_v2", "--dry-run", "--json",
                       data_dir=store).code == 0
            else fsm.run("instance", "migrate", row["instance_id"],
                         "--to=case_review_v2", "--dry-run", "--json",
                         data_dir=store).json()
            for row in listed
        ]
        rendered = json.dumps(previews)
        report.true(
            "req/migrate_unmapped" in rendered,
            "the preview names the migration engine's own refusal code",
        )
        uncovered = [row["state"] for row in listed if row["state"] != "intake"]
        for state in uncovered:
            report.true(
                state in rendered,
                f"the preview names {state}, the state responsible for its exclusion",
            )
        clean = [p for p in previews if p.get("clean") is True or p.get("refusal") is None]
        report.true(
            len(clean) >= 1,
            f"the covered instances are reported as migratable ({len(clean)} of {len(previews)})",
        )


# --------------------------------------------------------------------------
# manual: the executor runs a real workflow unattended.
# --------------------------------------------------------------------------

def the_executor_validates_a_shipped_handler_table(report) -> None:
    checked = fsm.run("execute", "--check",
                      f"--handlers={fsm.example('order_lifecycle.handlers.json')}",
                      "--json").ok().json()
    report.equal(str(checked.get("ok")).lower(), "true",
                 "the shipped handler table validates")


def the_executor_settles_a_pending_effect_and_advances_the_instance(report) -> None:
    """The shipped binary against a handler an operator would actually write.

    The suite proves the loop against a stub; this proves `fsm execute` itself,
    with a handler table on disk and a real subprocess behind it.
    """
    with fsm.Scratch("executor") as scratch:
        store = scratch.dir("store")
        fsm.run("machine", "add", fsm.example("order_lifecycle.json"), data_dir=store).ok()
        fsm.run("instance", "new", "order_lifecycle", "--request-id=x1", data_dir=store).ok()
        fsm.run("instance", "send", "inst-x1", "place", "--request-id=x2",
                data_dir=store).ok()
        before = fsm.run_json("instance", "show", "inst-x1", data_dir=store)
        pending = before.get("effects_pending") or []
        report.equal(len(pending), 1, "entering fulfilment left one effect pending")

        table = scratch.write("handlers.json", json.dumps({
            "format": "fsm.handlers/1",
            "handlers": [{
                "effect": "request_confirmation",
                "argv": ["/bin/true"],
                "timeout_ms": 30000,
                "on_ok": {"event": "pick", "payload": {}},
                "on_failed": {"event": "cancel", "payload": {}},
            }],
        }))
        fsm.run("execute", "--check", f"--handlers={table}", "--json").ok()

        def settled() -> bool:
            shown = fsm.run("instance", "show", "inst-x1", "--json", data_dir=store)
            return shown.code == 0 and not (shown.json().get("effects_pending") or [])

        landed = fsm.Executing(store, table).until(settled, timeout=90)
        report.true(landed, "the executor settled the effect unattended")

        after = fsm.run_json("instance", "show", "inst-x1", data_dir=store)
        report.equal(after.get("effects_pending") or [], [],
                     "the executor acknowledged the effect")
        history = fsm.run_json("instance", "history", "inst-x1", data_dir=store)
        kinds = [entry.get("kind") for entry in history.get("entries", [])]
        report.true("EffectAcked" in kinds, f"the ack is journalled: {kinds}")
        report.equal(after.get("leaf"), "shipping",
                     "the advance the table declares was applied")


def the_executor_exhausts_retries_onto_the_failure_path(report) -> None:
    """Backoff visible in the history, exhaustion firing the machine's own path.

    A stalled instance and an exhausted one look the same from outside unless
    something checks; the manual item existed because nothing did.
    """
    with fsm.Scratch("policy") as scratch:
        store = scratch.dir("store")
        fsm.run("machine", "add", fsm.example("order_lifecycle.json"), data_dir=store).ok()
        fsm.run("instance", "new", "order_lifecycle", "--request-id=p1", data_dir=store).ok()
        fsm.run("instance", "send", "inst-p1", "place", "--request-id=p2",
                data_dir=store).ok()

        table = scratch.write("handlers.json", json.dumps({
            "format": "fsm.handlers/1",
            "handlers": [{
                "effect": "request_confirmation",
                "argv": ["/bin/false"],
                "timeout_ms": 5000,
                "retry": {"attempts": 2, "backoff_ms": 1,
                          "on": ["nonzero_exit"]},
                "on_ok": {"event": "pick", "payload": {}},
                "on_failed": {"event": "cancel", "payload": {}},
            }],
        }))
        fsm.run("execute", "--check", f"--handlers={table}", "--json").ok()

        def exhausted() -> bool:
            shown = fsm.run("instance", "show", "inst-p1", "--json", data_dir=store)
            if shown.code != 0:
                return False
            view = shown.json()
            return not (view.get("effects_pending") or []) or \
                view.get("status") == "cancelled"

        landed = fsm.Executing(store, table).until(exhausted, timeout=90)
        report.true(landed, "the executor ran the handler to exhaustion unattended")

        history = fsm.run_json("instance", "history", "inst-p1", data_dir=store)
        kinds = [entry.get("kind") for entry in history.get("entries", [])]
        report.true(
            any(kind in ("EffectAttempted", "EffectAcked") for kind in kinds),
            f"the attempts and the settlement are journalled: {kinds}",
        )
        after = fsm.run_json("instance", "show", "inst-p1", data_dir=store)
        report.note(f"after exhaustion the instance is at {after.get('leaf')} "
                    f"({after.get('status')})")
        report.true(
            after.get("leaf") == "cancelled" or after.get("status") == "cancelled",
            "exhaustion fired the machine's declared failure path rather than stalling",
        )
        dead = fsm.run("execute", "--list-dead", f"--handlers={table}", "--json",
                       data_dir=store)
        report.true(dead.code == 0, "--list-dead runs against the resulting store")
        report.note(f"--list-dead reported {len(dead.json().get('dead_letters', []))} entries")


# --------------------------------------------------------------------------
# Not on the manual list, because the list predates plan 0017. Sealing is the
# operation that removes data, and the review that closed that plan found a
# path where it was removed silently — so it gets acceptance coverage against
# the shipped binary rather than only unit coverage.
# --------------------------------------------------------------------------

def a_sealed_store_archives_verifies_and_reopens(report) -> None:
    with fsm.Scratch("seal") as scratch:
        store = scratch.dir("store")
        archive = scratch.dir("archive")
        fsm.run("machine", "add", fsm.fixture_machine(CASE_REVIEW), data_dir=store).ok()
        fsm.run("instance", "new", "case_review", "--request-id=s1", data_dir=store).ok()
        fsm.run("instance", "send", "inst-s1", "docs_ok", "--request-id=s2",
                data_dir=store).ok()
        pending = fsm.run_json("instance", "show", "inst-s1",
                               data_dir=store).get("effects_pending") or []
        for index, effect in enumerate(pending):
            fsm.run("instance", "ack", "inst-s1", effect, "--outcome=ok",
                    f"--request-id=s-ack-{index}", data_dir=store).ok()

        preview = fsm.run_json("journal", "archive", f"--to={archive}", "--dry-run",
                               data_dir=store)
        report.true(preview.get("dry_run") is True, "the preview reports itself as one")
        report.true(os.listdir(archive) == [], "a preview writes nothing into the archive")
        cut = preview["sealed_through_seq"]

        sealed = fsm.run_json("journal", "archive", f"--to={archive}",
                              f"--before-seq={cut}", data_dir=store)
        report.equal(sealed["sealed_through_seq"], cut,
                     "the run sealed through the sequence the preview named")
        report.true(sealed.get("archive_id", "").startswith("sha256:"),
                    "the run reports the archive it wrote")
        report.true("MANIFEST" in os.listdir(archive), "the archive carries a manifest")

        # The store still opens, and still holds its instance.
        shown = fsm.run_json("instance", "show", "inst-s1", data_dir=store)
        report.equal(shown["instance_id"], "inst-s1",
                     "a sealed store still serves the instance it kept")

        # Verification: the middle verdict without the archive, the complete
        # one with it. A run that did not read the sealed bytes must never
        # report what one that did reports.
        without = fsm.run("journal", "verify", "--json", data_dir=store)
        report.true(without.code != 0,
                    "verifying without the archive is not a plain success")
        report.equal(without.json()["seal"]["verdict"], "prefix_not_presented",
                     "and it says the prefix was not presented")
        with_archive = fsm.run("journal", "verify", f"--with-archive={archive}",
                               "--json", data_dir=store)
        report.equal(with_archive.code, 0, "presenting the archive verifies")
        report.equal(with_archive.json()["seal"]["verdict"], "prefix_walked",
                     "and reports the prefix as walked")

        # A directory that is not this store's archive must not condemn it.
        elsewhere = scratch.dir("not-the-archive")
        wrong = fsm.run("journal", "verify", f"--with-archive={elsewhere}", "--json",
                        data_dir=store)
        report.equal(wrong.json().get("health"), "Ok",
                     "a mistyped --with-archive leaves the store reported healthy")

        doctor = fsm.run_json("doctor", data_dir=store)
        report.true("seal" in doctor, "doctor reports the seal a sealed store carries")


# --------------------------------------------------------------------------
# Also not on the list: `fsm machine test` is plan 0018's whole surface, and
# it is what a machine author runs most often.
# --------------------------------------------------------------------------

def machine_test_runs_cases_reports_a_delta_and_regenerates(report) -> None:
    with fsm.Scratch("cases") as scratch:
        machine = fsm.example("expense_approval.json")
        cases = fsm.example("expense_approval.cases.json")

        passing = fsm.run("machine", "test", machine, f"--cases={cases}")
        report.equal(passing.code, 0, "the shipped example case file passes")
        report.true("0 failed" in passing.out, "and reports every case as passing")

        one = fsm.run("machine", "test", machine, f"--cases={cases}",
                      "--case=an_amount_within_the_limit_goes_to_peer_review")
        report.equal(one.code, 0, "--case runs a single case")
        report.true("1 passed" in one.out, "and runs exactly one")

        # A deliberately wrong expectation fails, names the field, and exits
        # non-zero — the property CI depends on.
        broken = json.load(open(cases, encoding="utf-8"))
        broken["cases"] = broken["cases"][:1]
        broken["cases"][0]["expect"]["configuration"] = ["manager_review"]
        broken_path = scratch.write("broken.cases.json", json.dumps(broken, indent=2))
        failing = fsm.run("machine", "test", machine, f"--cases={broken_path}")
        report.true(failing.code != 0, "a diverging case exits non-zero")
        report.true("configuration" in failing.out and "peer_review" in failing.out,
                    "and names the field and what was actually observed")

        # Regeneration refuses an untracked file, which is the safeguard the
        # whole feature rests on.
        refused = fsm.run("machine", "test", machine, f"--cases={broken_path}",
                          env={"FSM_REGEN_FIXTURES": "1"})
        report.true(refused.code != 0, "regeneration refuses an untracked file")
        report.true("review" in refused.text.lower(),
                    "and the refusal says why review is the point")

        # The supersedes delta reports and never gates.
        old = fsm.fixture_machine(CASE_REVIEW)
        spec = _spec(old)
        digest = fsm.run_json("validate", old)["machine_id"].split("sha256:")[1]
        spec["name"] = "case_review_v2"
        spec["supersedes"] = {
            "machine": digest,
            "states": {s: s for s in
                       ["intake", "docs_review", "risk_review", "suspended",
                        "approved", "rejected"]},
            "context": {"visits": "ctx.visits", "notes": "ctx.notes",
                        "score": "ctx.score"},
        }
        new_path = scratch.write("v2.json", json.dumps(spec))
        core_cases = os.path.join(
            fsm.REPO, "crates/fsm-core/tests/fixtures/cases_v1.json"
        )
        delta = fsm.run("machine", "test", new_path, f"--cases={core_cases}",
                        f"--against={old}")
        report.equal(delta.code, 0,
                     "a completed delta run exits zero whatever it found")
        report.true("unchanged" in delta.out,
                    "and classifies each case")
        report.true("never a gate" in delta.out,
                    "and says plainly that it is a report")


# --------------------------------------------------------------------------
# manual: `cargo install --path crates/fsm-cli --locked && fsm version &&
# fsm docs spec`
# --------------------------------------------------------------------------

def the_installed_binary_reports_its_version_and_prints_the_spec(report) -> None:
    """The install itself happens in the image build; this is what it proves."""
    version = fsm.run("version").ok().out.strip()
    report.true(bool(version), f"the installed binary reports a version: {version}")
    with open(os.path.join(fsm.REPO, "Cargo.toml"), encoding="utf-8") as handle:
        declared = next(
            line.split('"')[1] for line in handle if line.startswith("version = ")
        )
    report.true(declared in version,
                f"and it matches the workspace manifest ({declared})")
    spec = fsm.run("docs", "spec").ok().out
    report.true(len(spec) > 10_000, f"`fsm docs spec` prints the spec ({len(spec)} bytes)")
    report.true("Appendix A" in spec, "including its error-code appendix")


# --------------------------------------------------------------------------
# manual: regenerate the decimal vectors and confirm they are byte-identical.
# --------------------------------------------------------------------------

def the_decimal_vectors_regenerate_byte_identically(report) -> None:
    import subprocess

    generator = os.path.join(fsm.REPO, "tools/gen_decimal_vectors.py")
    committed = os.path.join(
        fsm.REPO, "crates/fsm-core/tests/fixtures/decimal/generated_vectors.jsonl"
    )
    if not os.path.exists(generator):
        report.skip("the decimal generator is not in this image")
        return
    with fsm.Scratch("decimal") as scratch:
        first = os.path.join(scratch.path, "a.jsonl")
        second = os.path.join(scratch.path, "b.jsonl")
        for target in (first, second):
            subprocess.run(["python3", generator, target], check=True, timeout=300)
        with open(first, "rb") as a, open(second, "rb") as b:
            report.true(a.read() == b.read(), "two generations agree byte for byte")
        with open(first, "rb") as a, open(committed, "rb") as c:
            report.true(a.read() == c.read(),
                        "and agree with the committed fixture")
