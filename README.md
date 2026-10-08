# cic-primitive-engine

**A module does not interpret CIC schema. This engine interprets it,
materializes it, and proves it; the module works only on the closed data set.**

A Rust **library and CLI**, with a Go module alongside it (`go/`). Its
actual subject is the **materialized object** a CIC composition becomes
— not a pipeline that produces one. Generators — YANG, RESTCONF,
Kubernetes, Go, the Relay — consume that object, never the authored
YAML. See "The object" and "Division of labor", below, for a scope
correction to the pipeline framing this file used to carry.

> **Status: early.** `error` and `reader` are extracted and working.
> `canonical` implements section A's byte format, `conformance`
> implements section F's per-field comparator primitive
> (`compare`/`classify_field_value`, ported from `CIC-Relay`'s
> `compare.go`/`observation.go`), `collection` implements its
> collection-topology/element-identity primitive (`element_key`, ported
> from `collection.go`), `plan` drives both over a whole document
> (`evaluate`, ported from `conformance.go`), and `digest_projection`
> produces F5's own `conformance_plan_digest`/`observation_digest` from
> `plan`'s real types. The conformance/drift verdict artifact's own
> schema text lives in `docs/VERDICT-SCHEMA.md`, not in this crate's own
> types directly. `go/` carries a Go peer for all five Rust modules
> above, each independently verified against the same decided contract
> AND cross-checked against its Go peer through a shared differential
> corpus (`conformance/differential/`, all four of `comparator`/F6,
> `collection`/F7, `plan`/F8, `digest`/F9 landed) — not merely each
> independently verified in prose. Building `go/collection` caught a
> real, previously-uncaught negative-zero formatting divergence between
> `collection.rs` and real Go, fixed on the Rust side in the same pass;
> `go/plan` peers against `plan.rs`'s already-fixed multi-key path
> resolution, not `conformance.go`'s original single-split bug.
>
> **`materialized` exists.** `MaterializedObject`/`MaterializedField`/
> `FieldEvidence` (`engine/src/materialized.rs`) implement section B3's
> closed shape — refined past its own ASCII sketch; see "The object" and
> `docs/MATERIALIZATION-SPEC.md`'s B8 — plus the `try_new`
> custody-boundary check (`PRIMITIVE-IR.md`'s Complete property, enforced
> by construction). **`plan::evaluate` is wired to it at the root:**
> `intent`/`observed` are now `MaterializedObject`s, not bare `Value`
> trees, so a candidate must pass `try_new` before it reaches the
> comparator at all — closing the actual gap #39 named, not just building
> the type that could close it. Only the root changed; `conformance`/
> `collection` still take plain `Value`s, correctly, since they operate on
> already-resolved leaf values and collection elements, never the object
> itself (see `plan.rs`'s own doc comment for the exact boundary).
>
> **`go/materialized` exists** (`MaterializedObject`/`MaterializedField`/
> `FieldEvidence`/`IntentEvidence`, the Go peer of `materialized.rs`),
> including its own custody-boundary fixes (`NewMaterializedObject`
> validates every field's `Capability`/`FieldEvidence` and
> `canonical.IsValue`, and deep-copies on both construction and `Get`,
> since Go has no ownership transfer to lean on the way Rust does).
> `go/materialized` does not import `go/plan`'s `ConsumedField` the way
> the Rust side's `FieldEvidence` reuses `plan::ConsumedField` directly
> (same crate, no cycle to avoid) — Go packages can't do that once
> `go/plan` depends on `go/materialized`, so `go/materialized`
> reimplements the identical (coverage, value) state machine inline
> instead; see `go/materialized`'s own package doc comment.
> **`go/plan`'s own `Evaluate` is now wired through it too, the Go-side
> #41:** `intent`/`observed` are `materialized.MaterializedObject`s, not
> bare `map[string]interface{}`, so a candidate must already be
> custody-checked before `Evaluate` can run at all, and `Evaluate` no
> longer carries its own `canonical.IsValue` check (redundant once
> `NewMaterializedObject` already made that guarantee). `scalarCoverage`
> mirrors `plan.rs`'s own identically-named function (scalar paths:
> coverage authority = `FieldEvidence`; collection elements/containers:
> coverage authority stays `Observation`, for the same reason named
> there). **`conformance/differential/materialized/` now cross-checks
> `try_new`/`NewMaterializedObject` directly** — deliberately narrower
> than F6-F9's own groups: Rust's closed enums make an invalid
> `FieldEvidence`/`IntentEvidence` unconstructable in the first place,
> so the one thing both languages actually share and can diverge on is
> the Complete-property key-set comparison itself, which is what every
> vector there exercises. The existing `plan` differential corpus is
> unaffected, since it already fed both languages' `Evaluate` the same
> JSON fixture and each language already converted it on its own side.
> **A `role` module (Role short/long form, P0.2) lived
> here until 2026-10-07 and was removed** — it predated the scope
> correction below, had zero internal callers, and keeping it around
> would have let the tree imply a scope the contract now denies; see
> "What does not belong here" for why removal, not just naming the gap,
> was the right close. Nothing here is a stable API.

---

## Why a separate repository

The eight-atom primitive language is specified in
[`cic-primitives`](https://github.com/CentralInfraCore/cic-primitives): the
atoms, the grammar, the conformance rules. That repository is also a signed
schema-release pipeline — Vault, certificates, release bundles, provenance.

Those are two different jobs, and putting the engine inside either one is how
the previous attempt failed. The archived
[`cic-object-model`](https://github.com/CentralInfraCore/cic-object-model) grew
a specification, two implementations, a conformance corpus, a release gate and a
trust chain into one repository, and became impossible to move.

So the split is deliberate:

| repository | answers |
|---|---|
| `cic-primitives` | what the language *is*, and who signed this version of it |
| `cic-primitive-engine` | what a document *means*, mechanically, and what a module may receive |

## The object

**This repo's actual subject is the materialized object a CIC
composition becomes.** The object carries exactly the schema's keys, no
more and no less (`PRIMITIVE-IR.md`'s own **Complete** property, enforced
by `MaterializedObject::try_new`). Per key, `MaterializedField` holds
`capability` (B1, always present) plus `FieldEvidence` — the closed set
of legitimate value/coverage/provenance combinations section B3 actually
describes (`Intent(IntentEvidence)` | `Observation(ConsumedField)` |
`DerivedObservation(ConsumedField)`, where `IntentEvidence` is itself one
variant per provenance value, so only `Authored` can hold no value; see
B8 for why it's closed enums all the way down, not independent `Option`
fields).

**Correction, found while building the type:** an earlier version of
this section described each key as "a value wrapped in the Access
atom's own structure (`value`/`access`/`modify`/`inherit`/
`default_injection`/`conformance`)" — broader than what B3/B5 actually
decide. `access`/`modify`/`inherit`/`default_injection` are explicitly
**not** part of `MaterializedField` (B5: `default_injection` "is not a
second value held inside `MaterializedField`... it is computed at
response-construction time from the field's long-form descriptor," an
ACL/response-time concern the Relay/host owns, not this repo).

## Division of labor — scope correction

**This engine does not perform domain/schema-semantic validation —
Shape/Role algebra, reference resolution, or default/derivation
decisions.** Those are facts the object's ENVIRONMENT establishes and
writes onto it — whatever upstream component already schema-validated
and supplemented the data before it reaches this object:

```
environment                          this repo
───────────                          ─────────
Parse       (typed composition)  ─┐
Normalize   (short/long form,     │  hands this repo a
             defaults applied)    │  candidate object
Resolve     (references, cycles)  │
Validate    (Shape/Role algebra) ─┘
                                     boundary check (candidate really
                                       is a MaterializedObject?)
                                     Canonicalize (one byte form, for digests)
                                     conformance/collection/plan/
                                     digest_projection (given two
                                       already-materialized states --
                                       intent and observed -- does one
                                       conform to the other, and proves it)
```

Scoping OUT domain-semantic validation does not scope out a narrower,
load-bearing obligation this engine still owns: whether the candidate
object it receives actually satisfies the structural/semantic-state
invariants — the `Complete` property, long-form fields, valid
capability/coverage/provenance values — that canonicalization and the
F6-F9 proof machinery assume. `docs/MATERIALIZATION-SPEC.md`'s B7
already commits capability/coverage/provenance into the
`output_digest`-protected semantic claim; without a boundary check,
that digest would only prove "these bytes were canonicalized," not "a
valid `MaterializedObject` produced these bytes." F13/F14 (same spec)
already established this repo's own precedent for exactly this shape
of problem: a module validates untrusted input at its OWN boundary
rather than trusting an upstream stage (`ConformancePlan::validate`,
`PlanDigestProjection`'s own check) — the same principle applies one
level up, at the `MaterializedObject` boundary itself. **Built, not
merely named:** `MaterializedObject::try_new` (the Complete-property
check) and `FieldEvidence`/`IntentEvidence` (the closed,
illegal-states-unrepresentable shape invariants) — see "`materialized`
exists," above — plus `plan::evaluate`'s own wiring through this
boundary at the root.

This corrects, rather than extends, the pipeline framing this file used
to carry (`YAML bytes → Read → Parse → Normalize → Resolve → Validate →
Canonicalize`, all as this repo's own stages). `docs/MATERIALIZATION-SPEC.md`
still describes sections A through G at length as the historical
record of how the primitives' own semantics were decided — that
content stays correct about what a capability/coverage/provenance
fact, a comparator verdict, or a canonical byte means. What changes
here is narrower but load-bearing: *who performs* Parse/Normalize/
Resolve/Validate. It is the environment, not this repo.

One lesson from that environment-side work is still worth carrying
here, since it shaped how this repo's own `Complete` property got
decided: **a field that is absent cannot be skipped.** A checker in
`cic-primitives` once discovered nodes by looking for a `shape_type`
member, so a field that omitted `shape_type` was not reported as
invalid — it was invisible. Zero nodes examined, zero findings, green.
Discovery must never depend on the member whose absence is the defect
— wherever Parse actually runs.

## What does not belong here

Vault access · counter-signature policy · git and release workflow · domain
adapters · Kubernetes/OCI/network runtime logic · authorization decisions.
**Also out of scope under the correction above:** domain/schema-semantic
validation, reference resolution, default/derivation application, and
short-form expansion — the environment's job, before data ever reaches
this object. This does NOT include the structural boundary check named
above under "Division of labor" — that stays this engine's own
obligation, and it is built: `MaterializedObject::try_new`, wired into
`plan::evaluate`'s own entry point.

**A `role` module (Role short/long form, P0.2) lived here until
2026-10-07 and was removed, not just left named as a gap.** It was built
before this correction, under the old pipeline framing, was exactly this
kind of out-of-scope work (`Stage::Normalize`'s job), and had zero
internal callers — nothing in this crate ever used `expand_role`/`Role`
beyond re-exporting them. Keeping dead, out-of-scope code around to
avoid discarding the verification work that went into it would have
repeated the exact mistake this correction exists to fix: letting the
tree's contents imply a scope the crate's own contract denies. The code
and its verification notes (the `state_surface` default-authority quirk,
the upstream `cic-primitives#17`/`#18` cross-references) are not lost —
they are in this repo's own git history and in `cic-primitives`' own
`check_grammar.py`, the actual source of truth it was ported from.

A release verifier may call this engine to check the specs a bundle carries, but
the trust chain stays outside. *Is this primitive semantically valid* and *did it
come from someone you trust* are different questions; merging them is what made
the last repository unmovable.

## What was carried over, and what was not

Extracted from the archived `cic-object-model` after measuring model coupling
per file:

| file | lines | model references | |
|---|---|---|---|
| `value.rs` → `reader.rs` | 377 | **0** | carried over |
| `error.rs` | 114 | 1 | carried over, adapted |
| `canonical.rs` | 282 | 13 | not carried |
| `node.rs` | 202 | 25 | not carried |
| `materialize.rs` | 645 | 65 | not carried |

The reader refuses duplicate mapping keys at the event level, refuses anchors
and aliases *before* a tree exists (measured: 393 bytes of nested aliases expand
to 12,345,678 nodes in 2.7 s), enforces string keys and preserves insertion
order.

`error.rs` was adapted, not copied: its stage names spelled the archived model's
materializer, and its nineteen error codes named that model's ontology
(`E_ORIGIN_*`, `E_SEALED_*`, `E_UNKNOWN_PRIMITIVE`). Carrying those would have
reinstated the rejected model through the one file measured as free of it. Codes
are added as the stage that raises them is implemented, never in advance: a code
with no raiser and no vector is a claim, not a check.

A same-named `canonical.rs` exists today (section A's byte format,
`Stage::Canonicalize`) — not a contradiction of the table above. The archived
model's `canonical.rs` is the one "not carried"; this engine's own, written
fresh against `docs/MATERIALIZATION-SPEC.md`'s section A, shares none of its
code or its thirteen model references.

## Dependency pinning — closed 2026-10-07

`dependency.yaml` pins `cic-primitives` at `primitives/@v0.2.0` (commit
`960ee0f`, 2026-09-06). This was an open obligation since 2026-08-13: the
grammar this engine implements — the three-axis Role, the reference-as-
structural-annotation model, the closed authority/structural/lifecycle
cardinalities — wasn't in `primitives/@v0.1.5`, the tag that existed at the
time, so pinning there would have declared an origin that didn't contain what
was actually being implemented. `primitives/@v0.2.0` was confirmed to contain
that grammar by direct comparison against `schemas/atomic/role.yaml`, not
assumed from the version number, before pinning. `grammar_sha256`/
`grammar_schema_sha256` are recorded in `dependency.yaml` alongside the tag,
computed with `tools/compiler.py`'s own `get_sha256_b64` against the real
tagged files, per `docs/MATERIALIZATION-SPEC.md`'s D3/D1.1.

If anything is ever vendored from `cic-primitives` into this tree,
`imported_paths` makes the provenance gate enforceable with `--require
cic-primitives`.

## Conformance

`conformance/` holds language-independent vectors — `input.yaml` plus
`expected.yaml` — that any implementation must satisfy. The harness enforces two
properties about itself:

- an **empty corpus fails**, because zero assertions all pass;
- a group with **no accepted vector fails**, because a checker that rejects
  everything would satisfy a corpus of rejections.

Both were verified by breaking them on purpose and watching the suite go red.

The first two vectors are the cases where the archived repository's two
implementations disagreed and **no vector had caught it**: a duplicate mapping
key, and alias expansion. They became `INV-041` and `INV-042` there.

> A corpus proves what it contains. Only differential execution finds what
> nobody thought to write down.

That is also why `cic-primitives`' Python checker is kept as a **permanent**
differential oracle rather than a transitional one.

`conformance/differential/` is a second, separate layer: strict JSON
fixtures (not YAML — see its own README for why), checking whether the
Rust and Go sides of F6-F9 actually agree with each other, not just
each with the decided contract in prose. All four groups
(`comparator`/F6, `collection`/F7, `plan`/F8, `digest`/F9) are now
landed.

## Building

```bash
cargo build --workspace
cargo test  --workspace
cargo run -p cic-primitive-engine-cli -- read <file.yaml>
```

No local toolchain? `docker run --rm -v "$PWD":/w -w /w rust:1-slim cargo test --workspace`

The Go module (`go/`) is independent of the Cargo workspace above — its
own `go.mod`, no cgo, no dependency on this crate or on `CIC-Relay`.

```bash
cd go && go build ./... && go vet ./... && go test ./...
```

No local toolchain? `docker run --rm -v "$PWD/go":/w -w /w golang:1.25-alpine go test ./...`

## Licence

Apache-2.0.
