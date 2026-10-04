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

## Deliberately absent

- **A signature-related field.** Whether this schema ever reserves a slot
  for an externally-produced signature (parallel to `cic-primitives`'
  `release.sign`) is still open — C4's own call, not resolved by this
  document. Adding a field now would decide it by default, which is the one
  thing this file must not do.
- **A coverage projection.** Whether the receipt additionally carries a
  redundant `coverage` projection as an audit convenience is F1/F4's still-
  open, non-custody question. `output_digest` already commits to coverage
  (B7) regardless of whether this schema adds a second, redundant copy of
  it; omitted here pending that decision.
- **A schema-version field for the receipt schema itself.** `docs/
  MATERIALIZATION-SPEC.md`'s C section says the receipt needs "a formal,
  versioned schema" — found, while writing this out, to be ambiguous
  between "this document itself has a version" (true of any spec) and "every
  receipt instance carries its own schema-version field, the way
  `PRIMITIVE-IR.md` requires of the IR." Nothing already decided settles
  which. Not invented here — named as a new, small open question for C4,
  not answered by omission.

## Not decided here

- `conformance_plan_digest`/`observation_digest` — F5 already confirmed
  these are not receipt fields at all; they belong to the conformance/drift
  verdict artifact (F3), a different schema this document does not define.
- The verdict artifact's own wire layout — out of scope for this file,
  which is the materialization receipt only.
