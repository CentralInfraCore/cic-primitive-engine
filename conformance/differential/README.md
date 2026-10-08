# Differential corpus

A second corpus layer, distinct from `../reader/` and `../canonicalize/`
in one deliberate way: **strict JSON syntax, not YAML.**

## Why JSON here and YAML there

`../reader/` and `../canonicalize/` exist to pin `Stage::Read`/
`Stage::Canonicalize`'s own semantics — YAML scalar typing, explicit
tags, anchors/aliases, duplicate-key handling. YAML's own syntax is
the point of those vectors, so they are written in it.

This layer exists for a different, narrower question: **given the
same already-parsed semantic value, do the F6-F9 primitives
(`conformance`/`collection`/`plan`/`digest_projection`, and their Go
peers) agree on what it means?** That question has nothing to do with
how the value was spelled in its original source text. Mixing the two
would conflate two separate theorems — "the two YAML readers agree"
and "the two F6-F9 implementations agree" — into one test, so that a
failure can't tell you which one actually broke.

JSON is used here specifically **because it needs no YAML reader at
all**: it's a strict subset of YAML's own grammar (every valid JSON
document is already valid YAML), so the Rust side keeps reading these
fixtures with its existing `reader::parse` — no special-casing — while
the Go side, which has no YAML reader yet (that's `Parse`'s own job,
not yet built in either language), reads them with the standard
library's `encoding/json` directly. Neither side needs a new
dependency, and neither side's result depends on the other's parser.

**`Parse`, when it exists, gets its own differential corpus later,**
reusing `../reader/`'s and `../canonicalize/`'s existing YAML vectors
for exactly that cross-language YAML-semantics question — this layer
does not attempt to answer it now, and does not need to.

## Layout

```text
differential/
  comparator/    -- F6: conformance.Compare / ClassifyField / ClassifyFieldValue
  collection/    -- F7: collection.ElementKey
  plan/          -- F8: plan.Evaluate
  digest/        -- F9: digestprojection.*
  materialized/  -- MaterializedObject::try_new / NewMaterializedObject
```

One group per primitive, landed incrementally — matching how F6
through F9 themselves were ported one at a time, not all at once. All
four F6-F9 groups landed together first: every Rust F-module from F6
through F9 has been cross-checked against its Go peer through this
layer, not merely each independently verified against the decided
contract in prose. `materialized/` landed later, once both languages'
`MaterializedObject` existed (PR #40/#45) and were wired into
`evaluate`/`Evaluate` (PR #41/#46).

## Vector format

Each vector is a directory holding `input.json` and `expected.json`,
both strict JSON (no comments, no trailing commas — anything
`encoding/json` and this engine's own `reader::parse` both accept
unmodified). `comparator/`'s own fields:

```text
input.json:
  coverage        string   "observed" | "absent" | "unobserved" | "unknown"
  intent_present  bool     whether the intent side declares the field at all
  intent          any      the intent-side value (ignored unless coverage == "observed")
  observed        any      the observed-side value (ignored unless coverage == "observed")
  compare         string   "exact" | "numeric"

expected.json:
  verdict         string   the one FieldVerdict both languages must produce
  why             string   human-readable context, not compared
```

Every vector here was ported from `conformance.rs`'s own
`classify_field_value_mirrors_go`/`unknown_coverage_classifies_like_
unobserved` tests (themselves already checked against real Go output
earlier in this effort) — not invented fresh for this corpus.

`collection/`'s own fields:

```text
input.json:
  topology  string    "atomic" | "set" | "map"
  keys      [string]  key field names, byte-wise order doesn't matter (only meaningful for "map")
  elem      any       the element value

expected.json:
  identity  string    the one ElementKey result both languages must produce
  why       string    human-readable context, not compared
```

Ported from `collection.rs`'s own `element_key_mirrors_go`/
`float_values_get_the_same_identity_go_does`/
`nested_seq_and_map_values_match_go_v_formatting`/
`negative_zero_keeps_its_sign_unlike_canonical_float` tests. Includes
the exact negative-zero vector F12's own cross-language bugfix
(`collection.rs`'s `go_float_display` used to fold `-0.0` to `"0"`,
where real Go's `%v` prints `"-0"`) was found and fixed against —
landing it here means that specific regression now also fails loudly
for either language on its own, through the identical fixture, not
only through each crate's own hand-written unit test.

`plan/`'s own fields:

```text
input.json:
  intent        object        the intent-side document
  observed      object        the observed-side document
  observation   [object]      [{path: string, coverage: string}, ...] -- builds the Observation
  plan.scalars  [object]      [{path: string, compare: string}, ...]
  plan.collections  [object]  [{path, topology, keys: [string], elements: [{path, compare}]}, ...]

expected.json:
  object   string              the one ObjectConformance both languages must produce
  fields   map[string]string   the COMPLETE path -> FieldVerdict map Evaluate must produce --
                                full equality, not a subset: the plan alone determines
                                every path that can appear, so this also proves state-only
                                fields (declared nowhere in the plan) never leak in, without
                                a separate "forbidden fields" check
  why      string              human-readable context, not compared
```

Ported from `plan.rs`'s own `oci_conformant`/`oci_extra_state_fields_are_
not_drift`/`oci_drift`/`oci_not_comparable`/`oci_unobserved`/`oci_desired_
absent_is_conformant`/`unknown_coverage_flows_through_to_unobserved_
verdict`/`multi_key_collection_identity_resolves_back_to_its_element`
tests. The multi-key vector is F8's own inherited-Relay-bug fix
(`resolvePath`'s single-`"="`-split bug, PR #28) — intent and observed
deliberately differ, so a silently-failed path resolution (both sides
falling back to the same missing-path default) would produce a false
`CONFORMANT` instead of the `DRIFT` this vector actually requires.

`digest/`'s own fields:

```text
input.json, one of two shapes:
  plan      object   {scalars: [...], collections: [...]} -- same shape as plan/'s own "plan" field;
                      asserts conformance_plan_digest/ConformancePlanDigest
  consumed  [object] [{path: string, coverage: string, value: any?}, ...] -- value present
                      iff coverage == "observed"; asserts observation_digest/ObservationDigest

expected.json:
  digest  string   the one sha256:... value both languages must produce
  why     string   human-readable context, not compared
```

Every expected digest here was computed once from the real
`conformance_plan_digest`/`ConformancePlanDigest` and
`observation_digest`/`ObservationDigest` functions in **both**
languages and confirmed byte-for-byte identical before being pinned —
not invented, not hand-computed, and not generated from only one
language and trusted to match the other. `plan_digest_with_collection`
exercises F5's "each collection's keys sorted byte-wise" rule
end-to-end (declared `["zone", "name"]`, digested as if sorted to
`["name", "zone"]`); `observation_digest_absent_no_value` exercises
F5's "value present iff coverage == observed" rule by pinning the
digest a *missing* `value` field actually produces, not a `null`
placeholder for it.

`materialized/`'s own fields:

```text
input.json:
  fields         [string]  the candidate's own key names -- values don't matter for this
                            check (try_new's/NewMaterializedObject's own key-set comparison
                            never inspects them), so both harnesses wrap each as a trivial
                            Intent(Authored) field; only the names are exercised
  expected_keys  [string]  the key set the environment claims the schema declares

expected.json:
  accepted  bool    whether a candidate with this key set, against this expected key set,
                     must be accepted
  why       string  human-readable context, not compared
```

Deliberately narrower than F6-F9's own groups above: Rust's closed
enums make an invalid `FieldEvidence`/`IntentEvidence` unconstructable
in the first place, so there is no Rust side of a "reject a malformed
field state" comparison to run -- that class of check only exists on
the Go side (`go/materialized`'s own `Valid()`/`canonical.IsValue`
checks), already covered by its own unit tests, and is not eligible
for a cross-language vector at all. The one piece of logic genuinely
shared by both languages, and genuinely at risk of diverging, is the
key-set comparison (the Complete property) itself -- that is what
every vector here exercises.

## A named limitation: JSON numeric literals don't parse identically on both sides

The Rust side reads these fixtures through `reader::parse` (JSON is
valid YAML, so this needs no special-casing), which preserves the
int/float distinction from the literal's own spelling (`16` parses as
`Value::Int`, `16.0` as `Value::Float`). The Go side reads the same
bytes with `encoding/json` directly into `interface{}`, which always
produces `float64` for a JSON number regardless of whether it was
spelled with a decimal point — Go's own decoder does not preserve that
distinction the way Rust's reader does. Neither of this layer's two
comparator types is actually sensitive to this (the numeric comparator
treats int/float/BigInt identically by design; the exact comparator's
own "is this numeric at all" check doesn't care which numeric kind it
is, only whether both sides agree on being numeric or not) — so no
vector here depends on it — but a future vector author relying on
exact-comparator behavior that distinguishes `16` from `16.0` by
*spelling* would hit a real asymmetry this layer does not paper over.
