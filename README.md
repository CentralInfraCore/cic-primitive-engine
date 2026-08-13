# cic-primitive-engine

**A module does not interpret CIC schema. This engine interprets it,
materializes it, and proves it; the module works only on the closed data set.**

A Rust **library and CLI** that turns a CIC composition into validated,
materialized `PrimitiveIR`. Generators — YANG, RESTCONF, Kubernetes, Go, the
Relay — consume the IR, never the YAML.

> **Status: early.** `error` and `reader` are extracted and working. The stages
> above `Read` are not implemented. Nothing here is a stable API.

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

## The pipeline

```
YAML bytes
  → Read          strict document reading, before a tree exists
  → Parse         typed composition; structural positions are closed
  → Normalize     short forms expanded, applicable defaults applied
  → Resolve       references, inheritance chains, cycle detection
  → Validate      Shape algebra, Role algebra, cross-primitive rules
  → Canonicalize  one byte representation, for digests and caches
```

The stages are total and ordered: every rejection belongs to exactly one, and a
stage may only reject what the stages before it admitted.

**A field that is absent cannot be skipped.** The position it would occupy is
fixed at `Parse` and does not depend on the field being there. This is not a
theoretical concern: a checker in `cic-primitives` discovered nodes by looking
for a `shape_type` member, so a field that omitted `shape_type` was not reported
as invalid — it was invisible. Zero nodes examined, zero findings, green.
Discovery must never depend on the member whose absence is the defect.

## What does not belong here

Vault access · counter-signature policy · git and release workflow · domain
adapters · Kubernetes/OCI/network runtime logic · authorization decisions.

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

## Building

```bash
cargo build --workspace
cargo test  --workspace
cargo run -p cic-primitive-engine-cli -- read <file.yaml>
```

No local toolchain? `docker run --rm -v "$PWD":/w -w /w rust:1-slim cargo test --workspace`

## Licence

Apache-2.0.
