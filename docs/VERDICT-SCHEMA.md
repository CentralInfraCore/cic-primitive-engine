# The conformance/drift verdict — field-by-field

This is F3's artifact 3's schema **text** — the field-by-field layout for
the conformance/drift verdict, the sibling `docs/RECEIPT-SCHEMA.md` never
defines (its own closing line says so explicitly). It is a **contract**, in
`PRIMITIVE-IR.md`'s sense, matching `RECEIPT-SCHEMA.md`'s own convention:
not a sixth document inventing new facts, a layout for facts F1/F3/F5/F8/F9
already decided. Every field below cites the decision that requires it; a
field with no citation does not belong here.

Sibling artifact to both the materialization receipt (artifact 2, C) and
ProofTrace (artifact 1, E) — F3 already settled that these are three
distinct artifacts, never one merged into another. Produced by `evaluate`
(F8), over an intent/observed pair, a coverage lookup, and a compiled
`ConformancePlan` — not by the materialization pipeline that produces a
receipt.

## Fields

```text
field                     type                source
---------------------------------------------------------------------------
verdict_schema_version    integer             this document -- same
                                                       minimalism as
                                                       RECEIPT-SCHEMA.md's
                                                       receipt_schema_
                                                       version, extending
                                                       PRIMITIVE-IR.md's
                                                       Versioned property
                                                       to this artifact too

object                    string              F8 -- ObjectConformance:
                                                       "CONFORMANT" |
                                                       "DRIFT" |
                                                       "INCOMPLETE"
                                                       (conformance.go's
                                                       own three
                                                       constants)

fields                    map[string]string   F8 -- path -> FieldVerdict:
                                                       "CONFORMANT" |
                                                       "DRIFT" |
                                                       "OBSERVED_ABSENT" |
                                                       "UNOBSERVED" |
                                                       "NOT_COMPARABLE"
                                                       (observation.go's
                                                       own five constants)

conformance_plan_digest   string (sha256:…)   F5/F9 -- digest of the
                                                       executed
                                                       ConformancePlan
observation_digest        string (sha256:…)   F5/F9 -- digest of the
                                                       full observed
                                                       claim consumed
```

## `verdict_schema_version`: bump semantics

**Decided here, not left implicit: `verdict_schema_version` is a bare
integer, starting at `1`.** The Fields table above says it extends
`PRIMITIVE-IR.md`'s **Versioned** property the same way
`receipt_schema_version` does — but extending the property only commits
to the field *existing*; it does not by itself say when the value
changes.

**It MUST increment on any backward-incompatible change to the verdict
contract — not only field layout, but also requiredness,
cardinality/ordering, a field's semantics, or its digest/canonical
interpretation.** This is the identical rule `docs/RECEIPT-SCHEMA.md`
already states for `receipt_schema_version`, carried over rather than
re-derived: `RECEIPT-SCHEMA.md` itself needed a correction (PR #26) to
widen this past "field layout" alone, after `derived_values[].inputs`
kept its name and type while its *contract* (order-significant trace vs.
deduplicated set) changed incompatibly underneath that same name. Both
fields extend the identical `PRIMITIVE-IR.md` property for the identical
reason, so the same bump rule applies here from the start rather than
waiting to be caught by the same mistake twice.

## Required, cardinality, ordering

**Every field listed above is REQUIRED on every verdict instance; none may
be omitted-by-absence.** The same extension of `PRIMITIVE-IR.md`'s
**Complete** property `RECEIPT-SCHEMA.md` already applies to the receipt,
applied here to this sibling artifact: a verdict whose job is to be
audited cannot itself have ambiguous gaps.

**`fields` is REQUIRED and contains exactly one entry per path `evaluate`
actually classified — every scalar in `ConformancePlan.scalars`, and every
`{identity}/element-path` combination `elementKeys` produced for each
collection.** This is already `evaluate`'s own behavior (F8: every
`classify_at` call inserts into the result), restated here as a guarantee
this schema holds callers to, not a new requirement invented for the
document.

**`fields` needs no projection-level ordering rule — unlike `RECEIPT-
SCHEMA.md`'s `applied_defaults`/`derived_values`, which do.** Those are
lists (section A's A6 doesn't sort a `Seq`, so F5/C4 each had to add an
explicit "sort by path" rule before digesting one). `fields` here is a
genuine `Map`, not a list of `{path, verdict}` entries — and section A's
own A2 already sorts every `Map`'s keys byte-wise, unconditionally. Using
a map instead of a list isn't a stylistic choice; it means this field's
ordering is already settled by a rule this document doesn't need to
restate.

## Example

```yaml
conformance:
  verdict_schema_version: 1

  object: "CONFORMANT"
  fields:
    "/memory_gb": "CONFORMANT"
    "/network_interfaces/{name=nic-0}/subnet": "CONFORMANT"
    "/network_interfaces/{name=nic-1}/subnet": "CONFORMANT"
    "/shape": "CONFORMANT"

  conformance_plan_digest: "sha256:..."
  observation_digest: "sha256:..."
```

## Decided here: the procedural relationship to the receipt and to ProofTrace

F4 left this open after F3 drew the three-artifact distinction: *how* the
verdict relates procedurally to the receipt (C) and to ProofTrace (E) —
specifically, whether computing a verdict requires a receipt to already
exist, or whether they are independent outputs of the same
materialization call.

**Decided: independent. Computing a verdict does not require a receipt to
exist first, and producing a receipt does not require a verdict.** This
isn't a new design choice so much as a fact already true of the
implementation, named explicitly rather than left implicit: `evaluate`
(F8) takes `intent`/`observed`/an `Observation`/a `ConformancePlan` — it
has no parameter of the receipt's type, and nothing in its own
implementation calls into anything that produces one. Symmetrically, C2's
"every materialization call produces a receipt" has no dependency on a
verdict ever being computed; most materializations (intent-only, with
nothing yet observed to compare against) never will be. A receipt proves
*where a materialized value came from*; a verdict proves *whether an
observed value conforms to a declared intent* — two different questions
over two different inputs, which is exactly why F3 drew them as separate
artifacts in the first place. Nothing here requires a future caller to
sequence the two, and nothing should invent a dependency neither
artifact's own decided content actually has.

ProofTrace (artifact 1, E) is further still: it chains *workflow steps*,
not field-level semantics (F3) — a verdict is one possible *input* to a
ProofTrace entry (the step that ran `evaluate` would hash its own
input/output, same as any other step), but ProofTrace has no structural
awareness of `object`/`fields`/either digest, and this document does not
give it any.

## Not decided here

- Whether this artifact is itself signed, or carries a signature-related
  field — `RECEIPT-SCHEMA.md`'s own closed "no signature field" decision
  (C4) was specific to the receipt; this is a different schema, and
  nothing here extends that closure to it by default. Genuinely open,
  not silently inherited.
- An intent-digest-equivalent field (Go's own `IntentDigest`) in *this*
  schema. **Corrected 2026-10-09:** the original reasoning here —
  "it would need a `SpecDigest`-style expand+normalize step this
  engine's `Normalize` stage doesn't have yet" — is the same stale
  framing `docs/MATERIALIZATION-SPEC.md`/`docs/OPEN-QUESTIONS-GO-RUST.md`/
  `plan.rs` already corrected on 2026-10-08 (PR #48) and missed here:
  `Normalize` is permanently the environment's job, not a stage this
  engine will ever build. F16 (`digest_projection.rs`'s
  `materialized_object_digest`, PR #49, with its Go peer landed PR #50)
  has since closed the analogous gap for a `MaterializedObject` itself —
  but that digests the *object*, not this verdict artifact's own
  `object`/`fields`/plan/observation fields, so it is a different
  artifact's digest, not a field this schema was missing. This schema
  still carries no intent-digest field of its own; that remains true,
  just not for the reason originally given.

## Status update, 2026-10-09: the Go peer now exists for the computation, not yet a first-class artifact

This section used to list "how a Go peer of this artifact would be
produced" under "Not decided here" — **that line is now factually out
of date, not merely open, so it has been removed rather than corrected
in place.** A Go peer of `plan.rs`/`digest_projection.rs`'s logic exists
(`go/plan`, `go/digestprojection`), cross-checked against the Rust side
through `conformance/differential/plan/`+`digest/` since F11-F15,
including the `MaterializedObjectDigest` function added by PR #50.

**What exists, precisely, rather than the broader claim an earlier
draft of this note made:** Rust's `evaluate`/Go's `Evaluate` produce
equivalent semantic cores (`object`/`fields`, plus the `consumed` map
each language's own `ObjectVerdict`/`Evaluate` result carries) —
`plan.rs`'s own `ObjectVerdict` struct has exactly those three fields,
no more. `conformance_plan_digest`/`observation_digest`
(`ConformancePlanDigest`/`ObservationDigest` on the Go side) compute
the two digest fields separately, from a `ConformancePlan` and a
`consumed` map respectively, not from an `ObjectVerdict` itself.
**Neither language has a function or type that assembles this
document's full contract into one artifact** — nothing anywhere in
`engine/` or `go/` sets a `verdict_schema_version`, or bundles
`object`/`fields`/`conformance_plan_digest`/`observation_digest`
together into the single required shape above. Go/Rust computation
parity and digest parity are real and cross-checked; a first-class
verdict artifact builder is not built yet, in either language.
