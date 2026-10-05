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
  digest/        -- F9: digestprojection.*                     (not yet landed)
```

One group per F-primitive, landed incrementally — matching how F6
through F9 themselves were ported one at a time, not all at once.

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
