# The materialization receipt — field-by-field

This is C4's actual schema **text** — the field-by-field layout `docs/
MATERIALIZATION-SPEC.md`'s C section decided *which repo* hosts (this one)
and *which facts* must be present (C1–C5, D1, B7), but deliberately deferred
writing out until those closed. It is a **contract**, in `PRIMITIVE-IR.md`'s
sense — not a fifth document inventing new facts, a layout for facts already
decided elsewhere. Every field below cites the decision that requires it;
a field with no citation does not belong here.

Sibling artifact to the materialized data (C1), bound to it by digest, never
embedded in the `PrimitiveIR` tree. Produced on every materialization call
(C2), not only at release.

## Fields

```text
field                     type                source
---------------------------------------------------------------------------
receipt_schema_version    integer             this document -- which
                                                       version of THIS
                                                       contract (layout,
                                                       requiredness,
                                                       ordering, field
                                                       semantics), not
                                                       D1's schema_version
                                                       (the domain schema's
                                                       own version); see
                                                       "Decided, closing
                                                       C" below

grammar_sha256            string (sha256:…)   D1.1 -- atom-grammar rules
grammar_schema_sha256     string (sha256:…)   D1.1 -- instance-grammar schema

primitive_release         string              D1.2 -- release tag,
                                                       e.g. "primitives/@v0.2.0"
primitive_source_commit   string              D1.2 -- provenance.source_commit
primitive_build_hash      string (sha256:…)   D1.2 -- release.build_hash

schema_name               string              D1.3 -- logical/canonical name,
                                                       e.g. "cic:storage:
                                                       StorageResource"
schema_version            string              D1.3 -- e.g. "v1.2.0"
schema_digest             string (sha256:…)   D1.3 -- canonical schema digest

engine_identity           string              D1.4 -- which implementation,
                                                       e.g. "cic-primitive-
                                                       engine-rust"
engine_version            string              D1.4 -- that implementation's
                                                       own version

input_digest              string (sha256:…)   C1/BOUNDARY.md -- section A
                                                       digest of authored input
output_digest             string (sha256:…)   C1/BOUNDARY.md -- section A
                                                       digest of materialized
                                                       output (B7: includes
                                                       capability/coverage/
                                                       provenance)

applied_defaults          list                C5 -- DefaultEvidence, one
                                                       entry per defaulted
                                                       field
  .path                   string                     field path defaulted
  .rule                   string              C5 -- literal "schema-default"
  .value_digest            string (sha256:…)   C5 -- digest of the
                                                       substituted value

derived_values            list                C5 -- DerivedEvidence, one
                                                       entry per derived field
  .path                   string                     field path derived
  .rule                   string              C5 -- stable, versioned string
                                                       (name + "-vN")
  .inputs                 list[string]        C5 -- paths THIS invocation
                                                       actually read
  .value_digest            string (sha256:…)   C5 -- digest of the derived
                                                       value
```

## Required, cardinality, ordering

**Every field listed above is REQUIRED on every receipt instance; none may
be omitted-by-absence.** This isn't new — it extends `PRIMITIVE-IR.md`'s
already-decided **Complete** property (*"No member is optional-by-omission.
If a value is absent, the IR says so explicitly and says why"*) to this
sibling artifact, consistent with C2 (every successful materialization
produces a receipt). A receipt whose job is to be audited cannot itself
have ambiguous gaps.

**`applied_defaults` and `derived_values` are REQUIRED arrays, never
omitted, `[]` when there is nothing to report.** An omitted key is
ambiguous between "this engine doesn't populate this evidence" and "zero
entries were produced this call" — the same ambiguity `PRIMITIVE-IR.md`'s
Complete property exists to forbid. `[]` states the second thing
explicitly; omission would state neither.

**Ordering, stated explicitly because section A does not supply it for
arrays — the same gap F5 found and fixed for the two verdict-artifact
digests, now fixed here before it recurs:** A6 preserves `Seq` order
exactly as given; it has no opinion on receipt-array order at all. Without
a projection-level rule, two conforming implementations could list the same
evidence in different orders and produce different canonical bytes for
semantically identical receipts.

- `applied_defaults[]`: sorted ascending by `.path`, raw UTF-8 byte order
  (A2's comparator, reused).
- `derived_values[]`: sorted ascending by `.path`, the same rule.
- `derived_values[].inputs[]`: **a new decision, not inherited from C5.**
  C5 says `inputs` is "the exact paths THIS invocation actually read" —
  that settles *what* the list means, not whether order or duplicates are
  significant; the one landed example (`effective_state` reading
  `admin_state`/`oper_state`) doesn't establish either way, since it never
  repeats a path. Decided here: `inputs` is a **deduplicated, sorted-
  ascending (byte-wise) path set**, not an execution-order trace — if a
  future derivation rule reads the same path twice, or reads two paths in
  an order that varies between calls without changing the result, that
  must not change the digest. If a rule is ever written where the read
  *order itself* is part of what must be audited, it needs a different
  field — this one is a set.

## Example

```yaml
materialization:
  receipt_schema_version: 1

  grammar_sha256: "sha256:..."
  grammar_schema_sha256: "sha256:..."

  primitive_release: "primitives/@v0.2.0"
  primitive_source_commit: "a1b2c3d..."
  primitive_build_hash: "sha256:..."

  schema_name: "cic:storage:StorageResource"
  schema_version: "v1.2.0"
  schema_digest: "sha256:..."

  engine_identity: "cic-primitive-engine-rust"
  engine_version: "0.1.0"

  input_digest: "sha256:..."
  output_digest: "sha256:..."

  applied_defaults:
    - path: "$.config.replicas"
      rule: "schema-default"
      value_digest: "sha256:..."

  derived_values:
    - path: "$.state.effective_state"
      rule: "effective-state-v1"
      inputs: ["$.state.admin_state", "$.state.oper_state"]
      value_digest: "sha256:..."
```

The nesting above (`materialization:` as the sole wrapper, everything else
flat beneath it) is a decision made *here*, not inherited: `BOUNDARY.md`'s
own sketch nested the four version facts under one `schema`/`engine` pair
of keys (`schema: {version, digest}`, `engine: {version}`) — but that sketch
pre-dates D1's finding that there are **four** independent identifiers, not
two, each varying independently of the others (D1's own point in choosing
"four, not one or two"). Nesting them back under shared keys would visually
imply a grouping D1 explicitly rejected, so they are flat top-level fields
instead — one key per independent fact, matching how independently each one
actually changes.

**`schema_digest` absorbs the table's old `schema.digest` row, found while
writing this out, not a separate field left over from it.** C3's field table
(`MATERIALIZATION-SPEC.md`) lists `schema.digest` as "decided, BOUNDARY.md"
*and separately* lists "version-identity fields... D1" as its own row —
read side by side, that looks like two different things. It is not:
`BOUNDARY.md`'s pre-D1 sketch (`schema: {version: v0.2.0, digest:
"sha256:..."}`) was gesturing at exactly what D1.3 later decomposed properly
— a bare version-plus-digest pair, missing the logical name D1.3 adds and
conflating a schema-version-looking string (`v0.2.0`) with what is actually
D1.2's primitive-release tag format. D1.3's three-part group (`schema_name`/
`schema_version`/`schema_digest`) supersedes it; there is one schema-digest
field here, not two.

## Decided, closing C: the three remaining extension/meta questions

These three were left open by C4's core-field-layout closure (PR #24).
Closing them here closes section C in full — every one of C1–C5, C4's
which-repo question, its core field layout, and these three extension
questions is now decided. Nothing in C is open any more.

**`receipt_schema_version` is added — decided yes.** `docs/
MATERIALIZATION-SPEC.md`'s C section says the receipt needs "a formal,
versioned schema," which, read literally, is ambiguous between the
*document* having a version and every *instance* carrying one.
Resolved by the same move this document already made for
requiredness: `PRIMITIVE-IR.md`'s own **Versioned** property (*"Every
IR document declares its version. A consumer that has not declared
support for that version must not be handed it"*) applies to this
sibling artifact too, produced by the identical pipeline (C1). A bare
integer, starting at `1`.

**Correction (review-caught on PR #26): "incremented when the field
layout changes" was too narrow — a reader could take it to mean the
*set of field names* is the only thing this version tracks.** This
document's own content already disproves that scope: `derived_values[].
inputs` kept its field name and type (`list[string]`) when this file
decided it is a deduplicated sorted set rather than an execution-order
trace (the "Required, cardinality, ordering" section, above) — the
*layout* never changed, but the *contract* did, incompatibly, for any
consumer that had assumed order was significant. **`receipt_schema_
version` MUST increment on any backward-incompatible change to the
receipt contract — not only field layout, but also requiredness,
cardinality/ordering, a field's semantics, or its digest/canonical
interpretation.** Still not a complex versioning scheme, the same
minimalism C5 already applied to `"schema-default"` as a literal,
unversioned v1 constant — a bare integer, bumped on any of the above,
nothing more elaborate. Named `receipt_schema_version`, not
`schema_version`, specifically to avoid colliding with D1.3's
already-existing `schema_version` field, which is the *domain* schema's
own version — a different fact entirely, found only because writing
the two side by side made the name clash obvious.

**No signature-related field — decided no, not merely deferred.** This
schema does not reserve a slot for an externally-produced signature,
parallel to `cic-primitives`' `release.sign`. Grounded in extending
C1's own reasoning one level further: C1 already decided the receipt
is a **sibling** artifact, bound by digest, never embedded in the data
it describes, specifically to avoid the archived model's regress
(`BOUNDARY.md`: embedding provenance as a node member meant every
`origin` needed an `origin` of its own, without end). Reserving a slot
*inside* the receipt for a signature *of* the receipt is a smaller
version of the identical mistake — the receipt would carry a fact
about its own future handling, not about the data it describes. If
this receipt is ever signed, the signature lives in a separate,
sibling artifact wrapping it (`SignedReceiptEnvelope{receipt_digest,
signer_identity, signature}`, parallel to how the receipt itself wraps
the materialized data) — never a field in *this* schema. This is a new
decision, not a recovered one; D-015's `release.sign` precedent showed
a schema *may* reserve such a slot, not that *this* one must.

**No redundant coverage projection — decided no, not merely deferred.**
F1/F4 already settled the custody question: coverage needs no receipt
projection to survive the module boundary, because `output_digest`
(B7) already commits to it as part of the materialized semantic claim.
What remained was a smaller "nice to have for audits" question — would
duplicating coverage into the receipt make auditing more convenient.
Decided no, on this document's own founding discipline (its own
opening line: *"a field with no citation [to a decision requiring it]
does not belong here"*): nothing has established a concrete need for
faster or receipt-local coverage access that re-deriving it from the
materialized tree doesn't already satisfy. If a real audit workflow
later needs that, it is a new, concretely-motivated decision for then
— not one to pre-empt now on spec alone.

## Not decided here

- `conformance_plan_digest`/`observation_digest` — F5 already confirmed
  these are not receipt fields at all; they belong to the conformance/drift
  verdict artifact (F3), a different schema this document does not define.
- The verdict artifact's own wire layout — out of scope for this file,
  which is the materialization receipt only.
