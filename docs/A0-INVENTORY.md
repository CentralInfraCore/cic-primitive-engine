# A0 inventory: `core/nexus/iac` vs. `cic-primitives`

```text
Status: findings complete, decisions not made.
This file answers A0.1-A0.4's classification question. It does NOT decide
section A-G of OPEN-QUESTIONS-GO-RUST.md -- those decisions still have to
be made, now with this inventory as input instead of a blank page.
```

Scope: every non-test `.go` file in `CIC-Relay/core/nexus/iac/` (10 files,
~1250 lines) read in full, cross-checked against the 8 `cic-primitives`
atomic schemas (`access`, `address`, `behavior`, `contract`, `event`,
`identity`, `role`, `shape`) and `features/feature-011-oci-provider/
iac-object-model.md`.

## A0.1 — Already primitive semantics (candidates to migrate into the lib)

| `core/nexus/iac` piece | What it does | `cic-primitives` counterpart | Note |
|---|---|---|---|
| `field.go` — `ExpandField`/`Field`/`FieldMode` | `key: VALUE` ↔ `key: {value, default, mode: {read,write,implemented,visible}}` — the short/long form expansion | `access.yaml`'s `key: value` ↔ `key: {value, access, modify, inherit, default_injection, conformance}` | **Not a 1:1 rename.** Relay's form is *simpler* — no ACL embedded, no `default_injection`, no `inherit`. `mode.implemented` (bool) partially covers `conformance`, but see A0.4 — it's missing a state. |
| `digest.go` (`SpecDigest`) + `number.go` (`canonicalNumber`) + `canonicaljson.go` | Canonicalize → SHA-256; number normalization has a **proven Go/Rust pair** (`number.go` ↔ `cic-canonical` crate, byte-identical vectors) | Section A's whole question | Directly answers most of section A1/A2. Number normalization has Rust parity; **key-ordering (`canonicaljson.go`) does not yet have a Rust peer** — that gap is real follow-up work, not a fresh design. |
| `collection.go` — `CollectionTopology` (atomic/set/map) + `ElementKey` | List-element identity by topology, explicit keys for `map` | `shape.yaml`'s `collection_variant` + `item_key` (confirmed this session, and in `primitives/@v0.2.0`'s release notes: "every list has a key... two or more key fields require an explicit `item_key`") | Strong, concrete mapping. `TopologySet`'s `ElementKey` is explicitly a placeholder "until the CIC Canonical Object Encoding lands" — i.e. it already knows it depends on section A. |
| `node.go` — `Value`/`ValueKind`/`Node{Value, Meta}` | `Value` is a discriminated, private-field type — a module can only build one through its constructors, so its kind/payload can't be inconsistent. `Node{Value, Meta}` itself, however, has **exported** fields (`Node{Value: ..., Meta: ...}` is a legal literal anywhere), and `Validate()` is an explicit, separate call — nothing in the type forces it to run. | `docs/BOUNDARY.md`'s `Materialized<T>`/`Validated<T>` | **Partial prior art for section E, not the full guarantee.** `Value` genuinely gives the same "cannot represent an inconsistent value" property `BOUNDARY.md` wants. `Node` does not yet give the stronger "an unvalidated object cannot reach a module" property — that gap is exactly section E's open question, not something this file already answers. Corrected 2026-10-03 after review caught the original overclaim. |
| `reference.go` — `FieldRef{Target, Kind}` | A field's value may be a pointer to another resource by name | `atomic_ref`/`aggregate_ref` (schema-structural references) | Partial match only — Relay's reference is **instance data** (a field's authored value points at another resource by name), `atomic_ref`/`aggregate_ref` are **schema-structural** (which kernel atom backs a field's category). Different axis; needs real reconciliation, not a rename. Borderline A0.1/A0.3. |

## A0.2 — Pure runtime mechanism (stays in Relay; the lib has no opinion)

| File | Why it's mechanism, not semantics |
|---|---|
| `loader.go` | Reads YAML specs into the graph type. I/O, not primitive semantics. |
| `source_file.go` / `source_git.go` / `source_upstream.go` | Three `IaCSource` implementations — local filesystem, git, HTTP upstream. Pure transport. |
| `validator.go` (`IaCValidator`) | Resolves `schemaRef`s against the Cabinet registry and checks graph-level dependency integrity. Relay-specific orchestration over its own graph type, not a primitive-semantics concern. |
| `core/nexus/drift` (`drift.go`, package `drift`) | A **consumer** of `iac.Evaluate` — folds per-field verdicts into a `SOFT_DRIFT`/`HARD_DRIFT`/`ReconciliableDrift`/`NONE` report for the drift engine. Downstream of the semantics, not part of them. |

## A0.3 — New semantics, no `cic-primitives` counterpart today

These are real gaps on the primitives side, found by checking all 8 atoms
— not just a naming difference waiting to be noticed.

1. **`sensitive.go` — secret placement/custody policy** (`class` / `exposure`
   / `backup` levels). Checked all 8 atomic schemas: none of `access`,
   `address`, `behavior`, `contract`, `event`, `identity`, `role`, `shape`
   models *where a secret may live* or *how it may be backed up*. `access`
   only models *who may read/write* — a materially different question
   (custody/placement vs. permission). This is a genuine missing primitive,
   not a mapping exercise.
2. **`observation.go` + `compare.go` — coverage and intent/state comparison**
   (`CoverageState`: observed/absent/unobserved; `CompareType`: exact/
   numeric; the `CONFORMANT`/`DRIFT`/`OBSERVED_ABSENT`/`UNOBSERVED`/
   `NOT_COMPARABLE` verdict set). `cic-primitives` atoms describe schema
   *shape* and *validation* (authoring-time), not an algorithm for judging
   whether an *observed* value conforms to a *declared* one. This is
   section F's whole territory (output symmetry) and today it exists only
   as Relay-internal semantics with zero primitives-side vocabulary.
3. **`field_id.go` — stable field identity surviving a path rename.**
   `identity.yaml` is explicitly **type**-level identity (`kind`/
   `namespace`/`version`/`base` — what an entity *is*), not field-level
   addressing within a schema. `address.yaml`'s `logical_id` is
   instance-level identity for a whole entity, not a sub-field path. Neither
   atom covers "this specific field, regardless of where it's renamed to."
   No existing atom is the right home for this without stretching one.
4. **`acl.go`'s evaluation *algorithm*** — see A0.4 below; flagged here too
   because the gap is not just the algorithm's richness but that
   `cic-primitives` has no formal evaluation model at all, just a raw list.

## A0.4 — Actual conflicts (same concept, modeled incompatibly)

Not naming differences — these need an explicit decision because the two
models disagree, not just spell things differently.

1. **Conformance is tri-state in `cic-primitives`, boolean in Relay.**
   `access.yaml`'s `conformance` is `implemented` / `not_implemented` /
   `deprecated` (this session's D-012/D-017 work, cic-schema-registry#160).
   `field.go`'s `FieldMode.Implemented` is a plain `bool`. **`deprecated`
   has no representation in Relay's model at all today.** A straight
   rename cannot reconcile this; closing section B without addressing it
   would silently drop a state cic-primitives already treats as load-bearing
   (D-012's hard-reject distinction depends on `not_implemented` being
   separable from `deprecated`, and Relay's bool can't separate them).
2. **ACL: flat OR-list vs. POSIX classes.** `access.yaml`'s `access`/
   `modify` are `CertPattern` lists, OR-combined, no deny, no classes.
   `acl.go` (`cic-acl/v1`) is POSIX-style: owner/group/other classes, named
   allow entries, a mask capping named+group, inheritance, "most-specific-
   class-wins" evaluation — a materially richer, structurally different
   algorithm. If the lib adopts Relay's ACL as-is, `access.yaml`'s
   `access`/`modify` lists need to become a defined *short form* of it
   (e.g. an unqualified list = "other" class entries), not just be read
   as-is — that mapping does not fall out automatically and needs to be
   specified, not assumed.
3. **`behavior` names two unrelated things.** Relay's `$cic.behavior` layer
   (`default`, `mode`, `compare`, `sensitive`) and `cic-primitives`'
   `Behavior` atom (`rpc`/`action`/`operation` definitions — entity
   operations) share one English word for entirely unrelated concepts. If
   the lib's documentation or schema borrows Relay's `$cic` layering
   verbatim, this collision will confuse every future reader who knows the
   `Behavior` atom. Needs explicit disambiguation (rename one on the lib
   side, or a called-out "these are unrelated, despite the name" note) —
   don't let it ride.
4. **`default_injection` has no Relay counterpart, and it's not clear it's
   a straightforward addition.** `access.yaml`'s `default_injection` is
   what an unauthorized *reader* sees instead of the real value — a
   per-reader, visibility-dependent substitution. Relay's ACL model has
   allow/deny but no substitution mechanism at all; a denied read is
   presumably just refused or omitted, not replaced with a placeholder.
   Whether this becomes a feature the lib adds to Relay's ACL model, or
   stays a `cic-primitives`-only concept the lib doesn't surface at
   runtime, is an open design choice, not a mechanical merge.

## What this does and doesn't settle

**Settled by this inventory:** which specific files/symbols are candidates
for A0.1 (migrate), A0.2 (leave in Relay), A0.3 (genuinely new, needs a
primitives-side decision), or A0.4 (needs explicit reconciliation, not a
rename) — replacing the placeholder mapping sketch in
`OPEN-QUESTIONS-GO-RUST.md`'s A0 section.

**Not settled:** any of sections A–G themselves. In particular:

- Section A (canonical representation) can now start from `number.go` +
  `cic-canonical` + `canonicaljson.go` instead of a blank page, but the
  key-ordering Rust peer still doesn't exist and has to be written.
- Section B (semantic state model) has to resolve A0.4 item 1
  (tri-state vs. boolean conformance) as part of closing it, or it will
  under-specify a state `cic-primitives` already relies on.
- Section E (boundary enforcement) has partial Go prior art now
  (`node.go`'s `Value`, constructor-gated and internally consistent) —
  worth reusing. `Node` itself is not an example of the full guarantee:
  its fields are exported and `Validate()` is an opt-in call, not a
  type-level force, so "an unvalidated object cannot reach a module" is
  still unanswered by the existing code, not already solved by it.
- A0.4 items 2 and 4 (ACL model, `default_injection`) are open design
  questions for whoever closes sections B/C/E, not resolved here.
