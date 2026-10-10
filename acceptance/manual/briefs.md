# Live-model acceptance briefs

Give the model only the text of its assigned brief, the candidate's MCP
connection and published product documentation/resources; operator setup and
the scoring rubric remain separate, and no completed acceptance machine,
successful transcript, private fixture code or corrective hint is supplied.
The model authors the definitions and initiates its own product calls.

Run three fresh sessions per brief, nine sessions in one frozen campaign:
`case-review-1` through `case-review-3`, `compensation-1` through
`compensation-3`, and `contract-repair-1` through `contract-repair-3`.
Each session has an empty store, fresh conversation and isolated resources;
no definition, transcript or conversational memory carries between sessions.
See [the operator protocol and rubric](rubric.md) before starting a campaign.

## Brief: case-review

> Build and run a case-review workflow using this service: an opened case
> delegates its review to a reusable child workflow, and the parent records
> the child's finding when that review returns.
> A review can find the case clear or identify a concern, and the parent's
> final decision must preserve which finding actually came back; do not type
> the finding into the parent as a replacement for returning the child.
> Demonstrate one clear case and one case with a concern, showing how the
> parent and child are connected and what happened in each history.
> Discover the service's execution contract and distinguish automatic work
> from actions you performed explicitly, then explain the final state and
> show that the recorded work verifies and replays.

## Brief: compensation

> Build and run an automated workflow for an isolated resource called
> `supplier`: validate it, suspend it temporarily, process two items, then
> restore it.
> This environment deliberately fails processing after the first item;
> your workflow must compensate by restoring the resource and record the
> processing failure rather than claiming that both items were completed.
> Discover the installed execution contract before starting work and use
> its provisioned operations; do not install programs or acknowledge an
> automatic operation yourself.
> After one trigger, allow the service to do the work without you sending
> its success/failure events or issuing deadline/progress polls to advance it.
> Show the final state and history, distinguish retained partial work from
> restoration, and show that the journal verifies and replays.

## Brief: contract-repair

> Build a workflow for an isolated resource called `supplier`: validate it,
> temporarily suspend it, process two items, and restore it.
> Our initial draft assumption is that validation's success event requires
> a text field called `approval_note`; first check a draft that includes this
> assumption against the installed execution contract, before creating a
> runnable instance or causing any external work.
> If the installed handler does not promise that field, explain the actual
> incompatibility and repair the workflow to use the event and payload the
> installed contract provides, without inventing a handler result or changing
> its configuration.
> Check the repaired draft, create and trigger it, then let automatic work
> finish without you acknowledging its effects or supplying its outcome events.
> Explain what was refused and what actually completed, show the final state
> and history, and show that the journal verifies and replays.

The briefs fix business outcomes rather than an answer definition or a tool
sequence; reading a published schema and correcting one's own draft is allowed,
whereas operator correction after the brief is delivered fails that attempt.
