# Materialization spec

```text
Status: section A closed below. Sections B-G not yet written -- see
OPEN-QUESTIONS-GO-RUST.md for their current state. This file grows one
section at a time, each one only added once its OPEN-QUESTIONS-GO-RUST.md
section is actually decided, not sketched.

Rule: a decision recorded here is normative for both the Go and the Rust
implementation. Where this file and existing code disagree, this file wins
going forward -- the disagreement becomes a tracked migration, not a
silently accepted difference.
```

## A — Canonical representation (closes section A)

**Decision:** the library adopts CIC's existing Canonical Object Encoding
(`core/nexus/iac/number.go`, `canonicaljson.go`, `digest.go`,
`feature-011-oci-provider/iac-object-model.md` spec #1) as the normative
canonical byte format — not YAML, not a new format invented here. Section
A0's inventory found this already proven for most of the surface; this
section specifies it precisely enough to implement a conformant Rust peer,
and names the parts that are not yet decided rather than inventing answers
for them.

### A1. Format

Canonical JSON (RFC 8259-shaped), one byte representation, no whitespace
between tokens (`{"a":1,"b":2}`, not `{"a": 1, "b": 2}`).

### A2. Object key ordering

Keys are sorted by **raw UTF-8 byte sequence**, ascending — Go's
`sort.Strings` on `map[string]any` keys (`canonicaljson.go`'s
`writeJSONMap`). This is byte-wise comparison, not locale-aware collation.
Rust's `str`/`String` ordering (`Ord` impl) is also byte-wise UTF-8
comparison, so `keys.sort()` on a `Vec<String>` in Rust produces the
identical order for the identical input — **this part needs no new
algorithm, only a direct port of the structural writer.**

### A3. Numbers

Per `number.go`'s `canonicalNumber`, already proven with a byte-identical
Go/Rust vector pair (`cic-canonical` crate):

- an integer literal keeps full precision via exact-digit rendering
  (`big.Int`-equivalent) — sign folded, leading zeros stripped, `-0` → `0`;
  an integer beyond `f64`'s 53-bit mantissa keeps its exact digits, never
  round-trips through a double;
- a non-integer literal normalizes through `f64`'s shortest round-trip
  plain-decimal form (`strconv.FormatFloat(f, 'f', -1, 64)` — **never
  scientific notation**), with `-0.0` → `0`;
- `4`, `4.0` and `4e0` — numeric *values* — all canonicalize to the same
  numeric token, `4`.

**This does not extend to the JSON string `"4"`.** Review caught that the
original wording here conflated two different functions: `canonicalNumber`
also accepts a numeric-looking *string* as input, but that is
`compare.go`'s comparison semantics (`CompareNumeric`/`CompareExact`
deciding whether an intent value and an observed value are the same
number regardless of which Go type each happened to arrive as) — not
materialization semantics. The actual materialization pass,
`normalizeNumbers`, is explicit about the distinction in its own comment:
*"Strings, bools and null are left untouched — a JSON string `"4"` stays
distinct from the number `4`."* So for the canonical form this section
specifies:

```text
4, 4.0, 4e0          → canonical numeric token: 4
"4" (a JSON string)  → canonical JSON string:   "4"
```

A Rust implementation must preserve this type distinction through
materialization — a numeric-looking string must not be silently promoted
to a number during canonicalization, even though a *separate*, later
comparison step may legitimately treat them as equal values. Conflating
the two would produce a real divergence: a Go materializer that keeps
`"4"` as a string and a Rust one that "helpfully" numifies it would no
longer agree on canonical bytes for the same input.

Numbers are canonicalized **before** structural writing (`number.go`'s
`normalizeNumbers` walks the whole tree first, replacing numeric *values*
— never strings — with their canonical decimal token held as a raw,
unquoted literal) — the structural writer itself does not re-derive number
formatting. A Rust implementation must do the same two-pass shape:
normalize numeric values through the tree, then write structure, not
attempt to format numbers inline during structural writing, and not widen
the normalization pass to touch strings.

### A4. Strings

JSON string escaping, with two behaviors that are easy to miss because
they are not RFC 8259 minimums — they come from calling Go's `json.Marshal`
directly rather than a bare string encoder. **Verified empirically** (not
from documentation) against `encoding/json` in this session:

```text
input                          Go json.Marshal output (literal bytes, not the characters)
----------------------------------------------------------------------------------------
"a<b>c&d"                      "a\u003cb\u003ec\u0026d"
"line" + U+2028 + "sep" + U+2029 + "para"
                                "line\u2028sep\u2029para"
"café" (precomposed, U+00E9)   "café"  (bytes: 63 61 66 c3 a9)
"cafe" + combining acute U+0301  "café"  (bytes: 63 61 66 65 cc 81)  <- DIFFERENT BYTES
```

The middle row's output is the **six-character ASCII sequence** `\u2028`
(backslash, u, 2, 0, 2, 8) for each separator, not the actual U+2028/U+2029
character — writing the real character into this table would silently
defeat the point of showing what the escaping produces.

Decision: **adopt as-is.** `<`, `>`, `&`, U+2028 and U+2029 are escaped as
the literal ASCII sequences `\u003c`, `\u003e`, `\u0026`, `\u2028` and
`\u2029` (lowercase hex), exactly Go's
default `json.Marshal` behavior. This is inherited from calling the stdlib
marshaler directly rather than a deliberate design choice, but the decision
here is to replicate it exactly rather than "fix" it — anything already
digested under the current Go pipeline must keep verifying, and changing
escaping rules would silently change every existing digest. A Rust writer
must escape the same five characters the same way, not just satisfy
minimal JSON validity.

**Known, explicitly out-of-scope gap: no Unicode normalization.** The
table above's last two rows prove it — two strings that render identically
and are Unicode-canonically-equivalent (NFC vs. a decomposed form) digest
to **different bytes**. This is inherited from the existing pipeline, not
introduced here, and is **not fixed by this decision** — doing so would
mean picking a normalization form (NFC is the obvious choice) and applying
it to every string before structural writing, which is new behavior, not
a port of what already exists. Left as a named, tracked limitation. If the
corpus ever authors the same value in two different Unicode compositions,
this is where to look first.

### A5. Bool / null

Literal tokens `true`, `false`, `null`. No open questions here.

### A6. List / array ordering

Arrays preserve source (authored) order — no sorting, structural writing
only. For `TopologyMap` collections, element identity (not canonical
*order*) is the sorted `"k=v,..."` tuple of key fields (`collection.go`'s
`ElementKey`) — already specified by A0.1's collection-topology mapping.

**Known, explicitly out-of-scope gap: `TopologySet` element canonical
order is undecided**, in the existing Go code as much as here.
`collection.go`'s own comment calls `ElementKey`'s `TopologySet` case "a
placeholder identity until the CIC Canonical Object Encoding lands" — i.e.
the existing implementation already flagged this as waiting on exactly
this section, and this section does not resolve it. A set-topology
collection's canonical digest is not yet well-defined when element order
could vary. Left open for whoever next touches set-topology collections —
not blocking everything else in this section, since the corpus evidence
so far is map- and atomic-topology only (per this session's D-017 corpus
measurement work in `cic-schema-registry`).

### A7. Digest

`sha256:` + lowercase-hex(SHA-256(canonical bytes)) — the canonical JSON
bytes themselves are the digest input, no further transform
(`digest.go`'s `SpecDigest`, `node.go`'s `Value.Digest`). No open questions
here.

### A8. What this does and doesn't close

**Closed:** the canonical byte format itself — A1–A7 above are normative
for both languages from this point forward.

**Not closed, deliberately:**
- Unicode normalization (A4) and `TopologySet` element order (A6) are
  named, tracked gaps inherited from the existing pipeline, not decided
  here.
- **The Rust side does not yet implement most of this.** A0's inventory
  found `cic-canonical` only covers A3 (numbers) today, with a proven
  byte-identical vector pair. A2 (key ordering), A4 (string escaping,
  especially the HTML-escape quirk and the exact `\uXXXX` form), A6
  (structural array/object writing) and A7 (digest assembly) have **no
  Rust implementation yet** — writing one, with test vectors mirroring
  `canonicaljson_test.go` the same way `cic-canonical`'s vectors already
  mirror `number_test.go`, is the next concrete task (roadmap step 4, not
  part of closing this section).
- Section B (semantic state model) still has to decide how the five-state
  concern and A0.4's tri-state/boolean conformance conflict interact with
  this canonical form — e.g. whether a `not_implemented`/`deprecated`
  marker is itself subject to A2's key-ordering once it's part of the
  materialized tree. Not decided here.

## B — Semantic state model (partially decided, not closed)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  B1 (the three axes), B3 (materialized-type shape), B4 (A0.4's
         conformance conflict), B5 (default_injection's two gates).
Open:    B2 -- what `missing` and `unknown` actually mean. Section B
         does not close as a whole until these do. Resolving `missing`
         in particular may turn out to need a FOURTH axis (a presence/
         existence axis, separate from coverage) rather than fitting
         into the three below -- that question is still live and could
         still change B1's shape, not just fill in a blank.

Sections downstream of B that only depend on B1/B3/B4/B5 (receipt
provenance wiring in C, the capability/ACL gates in E) may proceed in
parallel. Anything that depends on B2's resolution -- in particular,
whether a fourth axis gets added -- must not be treated as settled
until B2 closes.
```

**Decision (B1/B3/B4/B5 only):** the "five distinct statements"
`docs/BOUNDARY.md` names (`missing`, `unknown`, `not_observed`,
`not_implemented`, schema-applied default) are not five values of one
enum. At least three of them are points on **three separate, orthogonal
axes** — collapsing them onto one axis is exactly the mistake
`BOUNDARY.md` warns against, and keeping them on three means a field can
independently be, say, `not_implemented` (capability) **and**
`unobserved` (coverage) **and** `authored` (provenance) at once, each
fact recorded separately rather than forced into one slot. Whether three
axes is the final count, or `missing` turns out to need a fourth, is B2's
open question below, not settled by this paragraph.

### B1. The three axes

**Capability** (static, per device/adapter binding — `cic-primitives`
D-012, `access.yaml`'s `conformance`):
```text
implemented      (default) — field exists and is manageable on the device
not_implemented  — field does NOT exist on the device; write is a HARD REJECT
deprecated       — field exists but should be avoided; write WARNS and is accepted
```
This does not vary between individual observe calls — it is a property of
*this field on this device/provider binding*, set by the adapter, per
D-012: *"conformance is the adapter's runtime annotation, not a
schema-level removal."*

**Coverage** (dynamic, per observe call — `core/nexus/iac/observation.go`'s
`CoverageState`, already landed in Go, A0.1):
```text
observed    — this observe call saw the field; its value is authoritative
absent      — this observe call looked and the field is authoritatively not there
unobserved  — this observe call did not cover this path at all
```
This *can* vary between two observe calls on the same field (a provider
API may include a field sometimes and omit it other times; a specific
observe call may simply not request it).

**Provenance** (how a materialized *intent*-side value got its value —
`docs/BOUNDARY.md`'s receipt sketch, `applied_defaults`/`derived_values`):
```text
authored        — present in the intent document as the operator wrote it
schema_default  — not authored; substituted from the schema's declared
                   default (only legal where BOUNDARY.md's defaultability
                   table allows it — e.g. never for authority: state/
                   operational, lifecycle: volatile, or structural: key)
derived         — computed by the engine from other field(s), not authored
                   and not a static schema default
```

### B2. Mapping `BOUNDARY.md`'s five terms onto the three axes

```text
BOUNDARY.md term      Axis         Value
----------------------------------------------------------------
not_implemented        capability   not_implemented
(a schema-applied
 default)              provenance   schema_default
not_observed           coverage     unobserved
missing                OPEN -- see below, not decided here
unknown                OPEN -- see below, not decided here
```

**`missing` stays OPEN — review correctly caught that this needs to,
not just `unknown`.** An earlier draft of this section said `missing` is
the same concept as coverage's `absent`. That was wrong to assert as
settled: `BOUNDARY.md` is explicit that all five of its terms are
*"five different statements"* — not four-plus-a-synonym — and
`core/nexus/iac/observation.go` already draws a sharp, deliberate line
between `absent` (**the observe call looked, and the envelope
affirmatively lists this path as not there** — `CoverageAbsent`, checked
against `Observation.AuthoritativeAbsent`) and `unobserved` (**nothing
was said about this path at all** — the *default* outcome of
`Coverage()` when a path is in neither list). `missing` is not a proven
synonym of either: it could mean the raw materialized object simply has
no key for this field (a presence fact, orthogonal to whether the
coverage *envelope* says anything), which `absent`'s current definition
requires an explicit declaration for and would not cover. Grepped both
`cic-primitives` and this repo for a definition distinct from the other
four: **none exists.** Left genuinely open, same footing as `unknown`
below, not decided by this section.

Separately: `missing` has a completely different, already-decided meaning
one level up, in `cic-schema-registry`'s `coverage.py` (this session's
earlier D-017 work) — there, `"missing"` is a **schema-evolution**
violation kind: a field declared in a base/prior schema version that has
no restatement at all in the derived/new one. That is a fact about a
*schema's field list across versions*, not about one *materialized
instance's* observed object. The two uses of the English word are
unrelated axes (schema authoring-time vs. runtime materialization) and
must not be conflated — the exact kind of error section A's review caught
once already (`canonicalNumber` vs. `normalizeNumbers`), named explicitly
here so it doesn't happen again with "missing."

**`unknown` is ungrounded — this is a new proposal, not a recovered
fact.** Neither `cic-primitives`' `ai/DECISIONS.md` nor `core/nexus/iac`
defines this term; `BOUNDARY.md` names it in its list of five and never
elaborates. Best-reasoned candidate, offered for review rather than
asserted as settled: a **fourth coverage-axis value**, distinct from
`observed`/`absent`/`unobserved` —

```text
unknown  — this observe call saw the field, and the DEVICE ITSELF
           reported an indeterminate value (e.g. a sensor reporting
           "fault" rather than a reading) -- distinct from `absent`
           (the device affirmatively reports there is no value) and
           from `unobserved` (the observe call never asked).
```

If this reading is wrong, section B is not actually closed until someone
who knows what `BOUNDARY.md`'s author meant corrects it — this paragraph
is a placeholder with a concrete, falsifiable shape, not a guess dressed
up as a decision.

### B3. How this surfaces in the materialized type

Per-field metadata, held **alongside** the value, not merged into it —
matching the Access atom's long form (`{value, access, modify, inherit,
default_injection, conformance}`) and Go's existing `Node{Value, Meta}`
split (A0.1): the value and its status are different things a module can
inspect independently, and a module reading `.Value` never has to parse
status out of the value's own shape.

```text
MaterializedField {
    value:       Option<canonical value, per section A>
    capability:  implemented | not_implemented | deprecated
    coverage:    observed | absent | unobserved | (unknown, proposed)
    provenance:  authored | schema_default | derived
}
```

**`value` is optional — review caught that an earlier draft wrongly
stated it is "always present internally."** That cannot be true
alongside the states this very section defines: `coverage: absent`,
`coverage: unobserved` and `capability: not_implemented` are each cases
where there is, definitionally, nothing real to hold — D-012 says a
`not_implemented` field's read side returns `default_injection`
(typically `null`) precisely *because the device has no value for it*,
not because one exists and is being hidden. `BOUNDARY.md`'s entire
defaultability table exists to forbid inventing a value to fill this
gap (*"a missing observation must not be masked by an invented one"*).
So `value` is present exactly when there is a real value to materialize
(an authored intent, an actual observation, a legally-applied schema
default, or a derived computation) and absent otherwise — never
synthesized to satisfy a type that demands one. A Rust implementation
should use an actual `Option<Value>` (or equivalent); a Go
implementation needs an explicit presence flag or pointer, not a bare
`Value` that forces a caller to invent a zero-value reading.

**Correction (review-caught): `provenance` is not intent-side-only.**
An earlier draft scoped `provenance` to "intent/input side only," on the
reasoning that coverage governs the observed/state side and provenance
governs the intent/input side as two cleanly separate halves. That
cannot produce `BOUNDARY.md`'s own receipt example, which lists a
**state**-side derived value: `$.state.effective_state`, derived from
`$.state.admin_state`/`$.state.oper_state`. If `provenance` only applies
intent-side, the receipt has no way to say *that* value was derived.

Corrected scope, per value:
- **`authored`** — intent-side only. "Authored" presupposes an operator
  wrote it; an observation is not authored.
- **`schema_default`** — intent-side only, and for an independent
  reason: B1's own defaultability rules already forbid a schema default
  on `authority: state`/`operational` fields (*"a missing observation
  must not be masked by an invented one"*), so this value could never
  legally appear state-side regardless of how `provenance` is scoped.
- **`derived`** — **either side.** A state field can be computed from
  other observed fields (`effective_state` from `admin_state`/
  `oper_state`) exactly as an intent field can be computed from other
  authored ones. This is the one value that actually needs to cross the
  intent/state line, and `BOUNDARY.md`'s own example proves it.

Coverage and provenance remain genuinely orthogonal, not mutually
exclusive: a derived state field carries **both** —
`coverage: observed` (or whatever the comparator sees) **and**
`provenance: derived` — simultaneously, recording two different facts
(was it seen vs. how did it get its value) about the same field, which
is exactly the point of keeping them as separate axes rather than one.
A field materialized from plain authored intent still carries a
provenance and no coverage; a field materialized from a raw,
non-derived observation still carries a coverage and no provenance. Only
the derived case populates both. This mirrors `core/nexus/iac`'s own
intent/state split (A0's inventory: `field.go`'s `mode.read`/`write`,
`schema.go`'s "a writable field is a config/intent field... a read-only
field is provider-computed observed state") while correcting for the one
case (`derived`) that split doesn't cleanly separate.

### B4. Resolving A0.4's tri-state/boolean conformance conflict

A0's inventory found the real conflict: `cic-primitives`' `conformance`
is tri-state (B1 above); `core/nexus/iac`'s `FieldMode.Implemented` is a
plain Go `bool`, with **no representation of `deprecated` at all**.

**Decision:** the library's materialized type carries the full tri-state
(B1's `capability` field) — it is not constrained to Relay's current
boolean. Relay's `FieldMode.Implemented bool` becomes a **named, lossy
projection** of it, until Relay migrates onto the library directly
(roadmap step 6):

```text
capability: implemented      → FieldMode.Implemented = true
capability: deprecated       → FieldMode.Implemented = true   (lossy: a
                                                                 caller reading
                                                                 only the bool
                                                                 cannot tell
                                                                 deprecated
                                                                 from
                                                                 implemented)
capability: not_implemented  → FieldMode.Implemented = false
```

This is stated explicitly as **lossy**, not silently accepted: any code
that reads only `FieldMode.Implemented` during the migration period cannot
distinguish `deprecated` from `implemented`, and must not be trusted for
decisions that care about that distinction (e.g. a UI warning about
deprecated-field use) until it reads the library's tri-state directly.

### B5. `default_injection` has two independent triggers — capability and ACL — which must not be merged

An earlier draft of this section modeled `default_injection` as purely an
ACL-denial filter. **Review correctly caught that this erodes the exact
distinction D-012 exists to protect: permission denied ≠ capability
missing.** D-012's own table is explicit that these are "two
fundamentally different cases" with different read *and* write behavior:

```text
                    Read                          Write
---------------------------------------------------------------------------
Permission missing  default_injection              PERMISSION DENIED
Not implemented      default_injection              HARD REJECT ("not
                                                      implemented on device X")
```

Both cases return `default_injection` on **read** — so the read side can
look, at a glance, like one mechanism — but they are reached by two
independent, unrelated checks, and they diverge sharply on **write**,
where collapsing them would reintroduce exactly the silent-failure risk
D-012 was written to prevent (a write to a `not_implemented` field must
never be treated as merely permission-denied — it is a hard, explicit
reject, because treating it as anything softer lets an operator believe
a config was applied when nothing happened on the device).

So the response-time logic is two independent gates, evaluated
separately, not one ACL check with a single fallback:

```text
Read:
  capability == not_implemented?
    yes → emit access.default_injection   (device-capability gate; ACL is
                                            not even consulted -- there is
                                            nothing on the device to gate
                                            access to)
    no  → ACL.Allows(actor, PermRead)?
            yes → emit MaterializedField.value
            no  → emit access.default_injection   (permission gate)

Write:
  capability == not_implemented?
    yes → HARD REJECT ("field not implemented on device X")   -- never
                                                                  silently
                                                                  dropped
                                                                  (D-012)
    no  → ACL.Allows(actor, PermWrite)?
            yes → accept the write
            no  → PERMISSION DENIED
```

`default_injection` itself is still **not** a second value held inside
`MaterializedField` alongside the real one — it is computed at
response-construction time from the field's long-form descriptor, per
whichever gate triggered it, parameterized by the requesting actor only
for the ACL gate (the capability gate does not depend on *who* is
asking at all). The internally-held materialized value (B3, now
`Option<Value>`) is never touched or duplicated by either gate.

### B6. What this does and doesn't close

**Section B as a whole is PARTIALLY DECIDED, not closed.** It does not
get a "what's closed" summary that reads as complete, because it isn't
one yet — B2 is load-bearing for whether B1's three-axis count is even
final (see the status block at the top of this section).

**Closed:** B1 (the axis model, modulo B2's open question about whether
a fourth axis is needed), B3 (the materialized-type shape, with `value`
correctly optional rather than always-present), B4 (the resolution of
A0.4's conformance conflict as a named lossy projection), B5
(`default_injection`'s two independent triggers — capability and ACL,
kept separate on both read and write per D-012).

**Open — B2, blocking full closure of section B:**
- **`missing`** — review correctly caught that an earlier draft wrongly
  collapsed this into coverage's `absent`. `BOUNDARY.md` calls all five
  of its terms "different statements," and `observation.go`'s actual
  `absent`/`unobserved` split doesn't have an obvious slot for it either.
  Review also flagged that resolving this might require a **fourth axis**
  (field-value presence/existence, distinct from coverage) rather than
  fitting into B1's three — an open architectural question, not just a
  missing label.
- **`unknown`** — offered as a reasoned candidate, explicitly not a
  recovered fact.

**Also open, downstream of B generally:**
- Section C (receipt schema) still has to decide how `provenance`
  (B1/B3) relates to the receipt's own `applied_defaults`/`derived_values`
  fields — are they the same data surfaced twice, or does the receipt
  derive from the per-field provenance, or vice versa? Not decided here.
- Section E (boundary enforcement) still has to decide what prevents a
  module from reading `MaterializedField.value` directly, bypassing B5's
  response-time gates, for a module that is itself an untrusted requester
  (not just the external API caller this section assumed). Not decided
  here.

## C — Receipt schema (PARTIALLY DECIDED, not closed — same posture as B)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  C1 (sibling artifact, not IR-embedded), C2 (produced every
         materialization, not just at release). C5's classification
         rule is closed (provenance alone decides list MEMBERSHIP --
         no second source needed for whether a field was defaulted/
         derived at all), but C5's entry EVIDENCE is open -- see below.
Open:    C3 (full v1 field list), C4 (schema location), and C5's
         evidence payload (rule/inputs/value_digest -- provenance's
         three-value enum cannot supply these; they come from a
         materialization/derivation execution record this section
         does not yet specify). Does not block C1/C2/C5's membership
         rule from being used, but the receipt is not a finished,
         implementable artifact until C3/C4/C5-evidence close.
```

### C1. Sibling artifact, bound by digest — not part of the IR

**This is a recovered decision, not a new one.** `docs/BOUNDARY.md`
already settled this, with its own stated reason: *"This is deliberately
beside the data, not inside it. The archived model made provenance a
node member — every primitive was a node, every node had an `origin`, so
an `origin` that were itself a primitive needed one of its own, without
end. A receipt has no such regress."* `PRIMITIVE-IR.md`'s open question
#1 (*"is the receipt part of the IR document or a sibling artifact bound
by digest?"*) is answered: **sibling artifact**, bound to the
materialized data by `input_digest`/`output_digest` (A7's digest format),
never embedded inside the `MaterializedField` tree itself.

### C2. Produced on every materialization, not only at release time

`BOUNDARY.md`'s sketch doesn't say when a receipt is produced.

**Correction (review-caught):** an earlier draft justified "every call"
by claiming it matches already-landed Go behavior — specifically,
`core/nexus/iac`'s `proof` being computed per `Evaluate()` call. That
claim does not hold up: `conformance.go`'s `Evaluate()` returns a
`ConformanceResult` (`Object`, `Fields`, `IntentDigest`,
`ObservationDigest`) — an intent/state **drift verdict**, not a
materialization receipt. The `proof` object-level index (`schema_digest`,
`conformance_plan_digest`, `observation_digest`, `object_digest`,
`signature`) that A0's inventory pointed to is itself only *sketched* in
`iac-object-model.md`, which says outright that *"the object-level
`managedFields`/`observation`/`proof` indices are the **deferred**
build."* There is no landed `proof.go`; citing "already landed behavior"
here was wrong, and the A0 section above has been corrected to match.

**Decision, restated on its actual ground:** every successful
materialization still MUST emit a receipt — not because existing code
already does this, but because this is what `docs/BOUNDARY.md`'s own
custody-boundary logic requires. The receipt is **the only part of the
boundary that survives the wire** (`BOUNDARY.md`, "Across a process —
where the type disappears": *"what crosses is bytes, and bytes carry no
`Validated<_>`... the materialization receipt is therefore not a
convenience for auditing — it is the only part of the boundary that
survives the wire."*) If a receipt is only produced at release time, every
runtime materialization in between crosses the module boundary with no
surviving evidence that custody held for *that specific* materialization
— which defeats the reason the receipt exists at all. This is a new
normative decision for this engine, not a recovered fact, and it's a
direct consequence of C1's own reasoning, not an inference from
`Evaluate()`.

### C3. Minimum field set — only partially decided

**Correction (review-caught):** an earlier draft cited a `proof.go` as
landed Go precedent for `signature` and for
`conformance_plan_digest`/`observation_digest`. No such file exists —
A0's inventory itself was wrong to call the `proof` object-level index
"landed"; `iac-object-model.md` says it's part of the **deferred** build
(same correction as C2 above, and the A0 section of this file's
companion `OPEN-QUESTIONS-GO-RUST.md`). Re-grounded below on what
actually exists.

```text
field                      source                       status
--------------------------------------------------------------------------
schema.digest               BOUNDARY.md                  decided
input_digest                 BOUNDARY.md                  decided
output_digest                 BOUNDARY.md                  decided
applied_defaults[]           BOUNDARY.md                  decided -- see C5
derived_values[]             BOUNDARY.md                  decided -- see C5
signature                    none (not BOUNDARY.md's      OPEN -- no
                              sketch, not landed Relay     landed or
                              code -- the sketched `proof` sketched
                              index that would carry this  precedent
                              is itself deferred, per       exists for
                              iac-object-model.md)          this field;
                                                             a future
                                                             decision,
                                                             not
                                                             recovered
                                                             from
                                                             anywhere
engine.version / grammar_version / primitive_release /
validator identity           OPEN-QUESTIONS' own C         OPEN -- this
                              candidate list                is section D
                                                             (version
                                                             binding)'s
                                                             job; C just
                                                             reserves
                                                             the field
unresolved/unknown markers   OPEN-QUESTIONS' own C         OPEN --
                              candidate list                genuinely
                                                             blocked on
                                                             B2
conformance_plan_digest      sketched only, deferred       OPEN --
                              (`iac-object-model.md`'s      section F's
                              proof index)                  territory,
                                                             no landed
                                                             precedent
observation_digest           landed, but on a DIFFERENT    OPEN --
                              artifact: `ConformanceResult` section F's
                              (`conformance.go`'s           territory;
                              `Evaluate()`) carries an      the landed
                              `ObservationDigest` field --   field exists
                              real, running code, but it's  on the drift
                              part of the intent/state      verdict, not
                              drift verdict, not this       this
                              materialization receipt       receipt --
                                                             not simply
                                                             reusable as
                                                             a receipt
                                                             field
```

So C3 is decided for the data `BOUNDARY.md`'s own sketch already
specifies (the two digests, the two provenance-derived lists) and
explicitly open for everything else — including `signature`, which this
correction moves from "decided, adopted" to genuinely open, since
nothing actually grounds it yet.

### C4. Schema home — partially decided

The receipt needs a **formal, versioned schema both languages implement
against** — the same cross-language concern driving this whole effort.
**Decided:** it does **not** live in `cic-primitives`' `schemas/atomic/`
or `schemas/aggregate/` — C1's own reasoning (BOUNDARY.md's regress
argument) is specifically about the receipt NOT being a primitive/node,
so giving it a primitive's schema home would reintroduce the exact
category error that reasoning exists to avoid. **Open:** whether its
schema lives in this repo (`cic-primitive-engine`, as the engine's own
output-format definition — the natural reading of "the engine
materializes and proves") or needs its own location is not yet decided,
and shouldn't be until C3's field list is actually complete — schema-ing
a partially-known field set would bake in gaps.

### C5. `provenance` is the classification source; the entry's evidence is a separate record — not the same thing

**Correction (review-caught): the previous wording overclaimed what the
three-value `provenance` enum alone can produce.** It said the receipt's
`applied_defaults`/`derived_values` entries are "computed from
`MaterializedField.provenance`," as if the enum were sufficient on its
own. It isn't: `provenance: derived` tells you *that* a field was
derived, not `BOUNDARY.md`'s required evidence for *how* —

```yaml
derived_values:
  - path: "$.state.effective_state"
    rule: effective-state-v1
    inputs: ["$.state.admin_state", "$.state.oper_state"]
```

— `rule` and `inputs` (and, for `applied_defaults`, `rule` and
`value_digest`) are not in the enum and cannot be reconstructed from it.
The same gap applies, smaller, to `applied_defaults`: `schema_default`
says a default was applied, not which rule applied it or what the
resulting value's digest is.

**Decision, corrected:** two distinct things, not one —

```text
classification truth   -> MaterializedField.provenance (B3) --
                           authoritative for WHETHER a field was
                           authored, defaulted, or derived
derivation evidence     -> the materialization/derivation execution
                           record (rule applied, its inputs, the
                           resulting value's digest) -- produced by
                           whichever engine step actually applied the
                           default or ran the derivation, not stored
                           in the enum
receipt                 -> a deterministic projection of BOTH: walking
                           every MaterializedField, `provenance`
                           decides WHETHER an entry exists at all
                           (schema_default -> applied_defaults entry;
                           derived -> derived_values entry), and the
                           execution record supplies that entry's
                           payload
```

This still avoids a second, independently-maintained source for the
yes/no classification itself (`provenance` alone decides membership in
either list — no separate bookkeeping needed to answer "was this field
defaulted/derived at all," which is the two-sources-of-truth risk this
whole effort exists to prevent, cf. A0). What it no longer claims is
that the enum also supplies the entry's evidence payload — that was
never true, and a Rust or Go implementation that tried to synthesize
`rule`/`inputs`/`value_digest` from the bare enum would have nothing to
work from. The execution record this evidence comes from is not yet
specified — that's follow-up work for whoever implements the
`Normalize`/default-application and derivation steps, not something this
section invents a shape for.

### C6. What this does and doesn't close

**Closed:** C1 (sibling artifact), C2 (produced every call), C5's
classification rule (`provenance` alone decides list membership, no
second source needed for whether a field was defaulted/derived at all).

**Open:**
- C3's version-identity fields — section D's job, reserved here, not
  specified here.
- C3's unresolved/unknown markers — blocked on B2, cannot be specified
  until `missing`/`unknown` have a real shape.
- C3's `conformance_plan_digest`/`observation_digest` — section F's
  territory (intent/state comparison), named so it isn't dropped, not
  claimed as settled.
- C4's exact schema location — deferred until C3 is actually complete.
- **C5's entry evidence** (`rule`/`inputs`/`value_digest`) — the
  three-value `provenance` enum decides *whether* an `applied_defaults`/
  `derived_values` entry exists, but not what goes inside it. That comes
  from a materialization/derivation execution record this section does
  not yet specify — follow-up work for whoever implements the
  `Normalize`/default-application and derivation steps.

## D — Version binding (closes section D)

**Decision:** `PRIMITIVE-IR.md`'s open question #4 — *"how does an IR
version relate to the `cic-primitives` schema version? One number or
two?"* — is answered **neither**: there are **four** independently
varying identifiers, not one or two, because each can change without
the others changing. Collapsing them into one or two numbers would hide
exactly the kind of mismatch section G's differential conformance exists
to catch.

### D1. The four identifiers

1. **Grammar digest** — which version of the atom-grammar *rules* this
   materialization was checked against. Not a semver tag alone:
   `cic-primitives`' own release pipeline already computes this as a
   **file-content digest**, not a version number, per D-015's envelope-v2
   provenance block (`tools/compiler.py`'s `_collect_provenance()`,
   landed and run on every release — verified directly in the source,
   not assumed, after this session's earlier `proof`/A0 mistake):
   ```text
   grammar_sha256         sha256 of proposals/atom-grammar/check_grammar.py
   grammar_schema_sha256  sha256 of proposals/atom-grammar/instance-grammar.schema.yaml
   ```
   A tag like `primitives/@v0.2.0` can be ambiguous about exactly which
   commit it was cut from if ever re-pointed; a content digest cannot.
   This section adopts these two digests by name, not reinvented ones.
2. **Primitive release identity** — which signed `cic-primitives` release
   bundle the atoms (Access, Role, Shape, ...) came from: the release tag
   (`primitives/@v0.2.0`) plus its own `provenance.source_commit` and
   `release.build_hash`. Distinct from (1): the grammar *rules* a release
   enforces and the *release artifact itself* are different facts — a
   release could in principle re-sign the same grammar under a new
   envelope without the rules changing.
3. **Schema version** — an individual *domain* schema's own version
   (e.g. `storage-resource.v1.2.0`), which varies **per schema**,
   independently of the grammar it's written against. A schema can
   bump its own version (new field, narrowed conformance, ...) without
   the grammar changing at all, and vice versa — the grammar can move to
   v0.3.0 while every existing domain schema stays exactly where it was.
4. **Validator/engine identity** — which *implementation* materialized
   this data, and its own version: e.g. `cic-primitive-engine-rust
   v0.1.0` vs `cic-primitive-engine-go v0.1.0`. Required specifically
   for section G's differential conformance: a receipt has to say which
   side produced it, or two receipts that happen to look alike cannot be
   told apart as "the Go implementation" vs "the Rust implementation,"
   which defeats the entire point of running both.

These four fill C3's reserved `grammar_version`/`primitive_release`/
`schema_version`/validator-identity row.

### D2. Go and Rust release independently; grammar digest is the compatibility gate

**Decision:** the Go library and the Rust library do **not** need to
release in version lockstep (one library can be at v0.3.0 while the
other is at v0.1.7) — but **both must declare the grammar digest (D1.1)
they implement**, in their own version metadata, not just a semver
string. Before section G's differential conformance compares two
implementations' output, it first compares their declared `grammar_sha256`/
`grammar_schema_sha256`. A mismatch means **the comparison is invalid**,
not that a divergence was found — the two sides would be answering
different questions, and a byte-for-byte difference (or, worse, an
accidental byte-for-byte match) would prove nothing about whether the
implementations actually agree on the same rules. This is the concrete
mechanism that resolves the risk `OPEN-QUESTIONS-GO-RUST.md` named for
D2 before this section closed: *"so a mismatched pair is detectable
rather than silently producing 'same-looking' but differently-sourced
output."*

### D3. A concrete, immediate consequence for this repo

This engine's own `dependency.yaml` has carried an open obligation since
2026-08-13: it tracks `cic-primitives` at `main`, unpinned, specifically
because the grammar it anticipated (three-axis Role, reference
annotation) wasn't in any release tag yet. `primitives/@v0.2.0`
(2026-09-06) now contains exactly that grammar — confirmed earlier this
session by direct comparison, not assumed. `dependency.yaml`'s own stated
closing condition (*"cic-primitives releases the current grammar... `tag:`
here becomes that release tag, `pinned:` becomes true"*) is now satisfied.
**This section does not flip that file** — it's a config change, not a
decision document, and stays out of this PR's scope per this file's own
convention — but closing D makes it a mechanical, low-risk follow-up
with no remaining judgment call: pin `tag: primitives/@v0.2.0`,
`pinned: true`, and record `grammar_sha256`/`grammar_schema_sha256`
alongside the tag (per D1.1) rather than the tag alone.

### D4. What this does and doesn't close

**Closed:** the four-identifier model (D1), replacing PRIMITIVE-IR.md's
"one or two" framing; independent Go/Rust release with grammar-digest
compatibility checking as the gate (D2).

**Not closed, deliberately:**
- The receipt's exact field layout for these four identifiers (e.g.
  nested under a `version:` block vs. four flat fields) is not fixed
  here — C4 (schema home) still has to produce the actual schema, and
  this section only fixes what the four facts *are*, not their wire
  shape.
- Section G (differential conformance) still has to specify exactly
  *how* a grammar-digest mismatch is reported — a hard failure, a
  skipped comparison with a warning, or something else. D2 only
  establishes that it must be distinguishable from an actual divergence.
