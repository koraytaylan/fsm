---
id: elicitation-schema-derivation
title: "Elicitation Schema Derivation"
workstream: "0064"
kind: task
depends_on:
  - inbound-response-routing
gated: false
touches:
  - crates/fsm-cli/src/mcp/elicit.rs
  - crates/fsm-cli/tests/elicit_schema.rs
  - crates/fsm-cli/tests/fixtures/elicit/record.schema.json
status: done
merged_as: ""
---
# Elicitation Schema Derivation

MCP restricts an elicitation schema to a flat object of primitive properties, which is exactly the shape an event's declared `fields` already has — so the form a person fills in is generated from the machine's own types, and a decimal stays a string all the way through.

**Steps:**

1. In `crates/fsm-cli/src/mcp/elicit.rs`, implement `pub(crate) fn schema_for_event(m: &CompiledMachine, event: &str) -> Result<Value, ErrorObj>` producing a flat object schema from the event's declared fields.
2. Map declared types exactly as the architecture table states: `str` → `{"type": "string"}`; `int` → `{"type": "integer"}`; `bool` → `{"type": "boolean"}`; `{enum: "Name"}` → `{"type": "string", "enum": [variants…]}` in declaration order; `{decimal: "N"}` → `{"type": "string"}` with a description naming the exact fraction-digit count; `timestamp` → `{"type": "string", "format": "date-time"}`; `duration` → `{"type": "string"}` with a description of the accepted form.
3. **Decimals and timestamps are strings.** SPEC's rule that numerics are strings everywhere, and that a raw JSON number is `req/number_token`, applies here too. Emitting `{"type": "number"}` for a decimal would invite a client to return a float, which is the one value this engine refuses to accept anywhere. Write that reason in a comment.
4. Mark every field `required` unless the definition gives it a default, since `validate_event` demands the exact declared field set and an optional field would produce a payload the engine then rejects.
5. Carry each field's own documentation into its property `description`, so the form a person sees is the form the machine's author wrote.
6. Implement `pub(crate) fn payload_from_content(m: &CompiledMachine, event: &str, content: &Value) -> Result<Value, ErrorObj>` coercing the response through the **same** typed path an external payload takes — the `coerce_ctx_override`-style helpers in `store/json_helpers.rs` — so an elicited value and a sent value are validated identically and no second validation rule exists.
7. Refuse a nested object or an array anywhere in the derived schema. If a future field type cannot be expressed as a flat primitive, return an error naming the field rather than emitting a schema the protocol forbids.

**Tests:**

- `crates/fsm-cli/tests/elicit_schema.rs`: an event with one field of each declared type produces the exact documented schema, byte-compared against a committed fixture.
- A `{decimal: "2"}` field produces `{"type": "string"}` with a description naming two fraction digits — assert the type is **not** `number`.
- A `timestamp` produces a string with `format: "date-time"`.
- An enum field carries its variants in declaration order.
- Every property is `required` for an event whose fields have no defaults.
- The schema is flat: no property value contains a nested `object` or `array` type.
- `payload_from_content` coerces a valid response into a payload that `validate_event` accepts.
- A response with a raw JSON number for a decimal field is refused with `req/number_token`, exactly as an external payload would be.
- A response missing a required field is refused with `req/field_missing`; an extra field with `req/field_unknown`; a wrong scale with `req/field_scale`.
- An event with no fields produces a schema with an empty property set and elicits an empty form.

- **Done when:** `cargo test -p fsm-cli --test elicit_schema` passes every case above, decimals and timestamps are strings, coercion reuses the external payload path so the two validations cannot diverge, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `schema_for_event` turns an event's declared fields into the flat object of primitives the protocol allows, byte-compared against a committed fixture for an event carrying one field of every type. Decimals and timestamps are **strings**, for the reason SPEC gives everywhere else: a `number` invites a float, which is the one value this engine refuses to accept anywhere. An `int` is an `integer` — it has no scale to lose — and `payload_from_content` turns the number a conforming client returns back into the string the engine takes.

Everything else passes through untouched, including a number where the schema asked for a string: that is a client contradicting its own form, and it is refused as `req/number_token` exactly as an external payload would be. Validation is `validate_event` itself — the same function the send path calls — so an elicited value and a sent value cannot be judged by two rules. An answer carrying a key nobody asked for is kept rather than dropped, so it comes back as `req/field_unknown` instead of vanishing.

**Corrections.**

- *Step 4's "unless the definition gives it a default" has no case to cover.* A declared event field is a name and a type; there is no default mechanism, so every field is required, which is what `validate_event` demands anyway.
- *Step 5's per-field documentation does not exist to carry.* `FieldDecl` is `{name, ty}`. The descriptions that do exist are the ones the type justifies — the decimal's exact digit count and the duration's unit — and inventing prose from a field's name would be worse than saying nothing.
- *Step 7's runtime refusal is a compile error instead.* The match over declared types is exhaustive with no wildcard, so a field type that cannot be expressed as a flat primitive fails to build rather than failing at a client.
- *An internal event is refused here too.* Nobody can be asked to fill in a form for an event only the machine may raise, and 6403 should not have to be the first place that is noticed.
