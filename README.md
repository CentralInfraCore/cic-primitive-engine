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
> **The actual `MaterializedObject` this engine's own contract is
> about — Access-wrapped value + capability/coverage/provenance per
> key, exactly the schema's keys — does not exist yet.** Everything
> listed above operates on a generic value tree
> (`reader::Value`/`interface{}`) directly; building the real object
> type — including the boundary check that rejects a structurally
> invalid candidate before canonicalization or proof ever runs — and
> wiring `conformance`/`collection`/`plan` to read from it, is the next
> real step. `role` (Role short/long form, P0.2) predates the scope
> correction below and is not wired to anything. Nothing here is a
> stable API.

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
more and no less (`PRIMITIVE-IR.md`'s own **Complete** property): per
key, a value wrapped in the Access atom's own structure (`value`/
`access`/`modify`/`inherit`/`default_injection`/`conformance`), plus
three independent facts about it (section B3): capability, coverage,
provenance. A short-form authored key (`key: value`) and its long form
(`key: {value: value, access: inherit, ...}`) mean the same thing; the
object itself only ever holds the long form — the short form is an
authoring convenience that never survives into the materialized
object.

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
level up, at the `MaterializedObject` boundary itself. That check does
not exist yet, because the type it protects does not exist yet; it is
named here as an obligation this engine owes, not disclaimed as out of
scope.

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
obligation, just not yet built. `role`'s `expand_role` (Role short/long
form, P0.2) was built
before this correction and is exactly this kind of work; it stays in
the tree for now, named here rather than silently kept as if it still
belonged.

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

## Open obligation: the dependency is not pinned

`dependency.yaml` tracks `cic-primitives` at `main`, not at a tag. The grammar
this engine implements — the three-axis Role, the reference annotation, the
closed structural positions — is not in `primitives/@v0.1.5`, the newest tag.
Pinning there would declare an origin that does not contain what is being
implemented.

**This must be closed** when `cic-primitives` releases the current grammar: the
tag replaces `main`, `pinned` becomes true, and if anything is ever vendored,
`imported_paths` makes the provenance gate enforceable. Until then, nothing
downstream may treat this engine's behaviour as bound to a released grammar
version.

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
