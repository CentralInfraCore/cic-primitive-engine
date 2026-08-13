# The module boundary

**Nothing crosses into a module except a fully materialized, normalized and
validated data set.**

This is the engine's reason to exist. It is a custody boundary, not a
convenience: the states below must not be *representable* on the module side.

- raw YAML
- a partially interpreted document
- a structurally-checked but semantically-unvalidated object
- an object with defaults still unapplied
- an object with references still unresolved

## In-process

```rust
pub struct Materialized<T> { data: T, receipt: MaterializationReceipt }
pub struct Validated<T>(T);
pub type ModuleInput = Validated<Materialized<PrimitiveDocument>>;
```

Constructors are private to this crate. A module cannot manufacture one, so its
contract simplifies: every mandatory member is present, every applicable default
is substituted, every short form is normalized, every reference is resolved or
explicitly unresolved, every type and contract holds, and the document is bound
to a schema and engine version.

## Across a process — where the type disappears

**The type system does not survive serialization.** Relay-to-module, WASM,
subprocess, JSON: what crosses is bytes, and bytes carry no `Validated<_>`.

CIC is distributed, so this is the normal case, not the exception. The
[materialization receipt](#the-receipt) is therefore not a convenience for
auditing — it is *the only part of the boundary that survives the wire*, and it
must be bound to the canonical digest of the data it describes, or it can be
carried next to a different data set.

The archived object model showed the in-process half is achievable in Rust and
only approximately in Go, where a residue remained. That asymmetry is another
reason not to make the type the primary argument.

## Defaults are not uniform — they follow Role

Filling every absent value from the schema is wrong. `cic-primitives` splits
Role onto three orthogonal axes, and defaultability follows the axes:

| Role | defaultable? | why |
|---|---|---|
| `authority: config` | yes | the schema may define a desired starting state |
| `authority: state` | **no** | a missing observation must not be masked by an invented one |
| `authority: operational` | **no** | measurement cannot be manufactured by a schema |
| `lifecycle: derived` | by computation | deterministic derivation, not a schema default |
| `lifecycle: volatile` | **no** | absence here is itself data about freshness |
| `structural: key` | **no** | identity is never guessed |
| `structural: reference` | only under strict conditions | a defaulted reference still owes referential integrity |

If an adapter could not observe `power_state`, the result is not
`power_state: running` because that is the schema default. **`missing`,
`unknown`, `not_observed`, `not_implemented` and a defaulted value are five
different statements.**

## Two validations, not one

**Authoring validation** — over what was actually submitted: structure, no
forbidden members, no duplicate keys, no schema-owned value authored, correct
types, sealed/required respected.

**Materialized validation** — over what the module will actually receive, after
defaulting, normalization and derivation: every mandatory value present, every
applied default itself satisfying its contracts, references resolvable, no
contradictory members, Access and Role jointly consistent, the document closed
with nothing left unprocessed.

The second exists so that **defaulting cannot produce an invalid object**.

## The receipt

```yaml
materialization:
  schema:  {version: v0.2.0, digest: "sha256:..."}
  engine:  {version: v0.1.0}
  input_digest:  "sha256:..."
  output_digest: "sha256:..."
  applied_defaults:
    - {path: "$.config.replicas", rule: schema-default, value_digest: "sha256:..."}
  derived_values:
    - path: "$.state.effective_state"
      rule: effective-state-v1
      inputs: ["$.state.admin_state", "$.state.oper_state"]
```

The module receives complete values; the receipt says where each came from. An
audit can separate what was authored from what the schema added and what the
engine computed, and a schema-default change stays reproducible against the old
materialization.

This is deliberately **beside** the data, not inside it. The archived model made
provenance a node member — every primitive was a node, every node had an
`origin`, so an `origin` that were itself a primitive needed one of its own,
without end. A receipt has no such regress.

## Output is a new claim

```
ValidatedMaterializedInput → module → UntrustedModuleOutput
                                    → output schema validation
                                    → ValidatedObservation / ValidatedConsequence
```

A module's output does not inherit trust from its input. An adapter receives a
proven contract, talks to a real system, and returns a raw observation; that
observation is validated against the state/operational schema before it may
enter the CIC state or proof chain.
