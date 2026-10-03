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
    coverage:    observed | absent | unobserved | (unknown, proposed)   (state/output side only --
                                                                           see note below)
    provenance:  authored | schema_default | derived                     (intent/input side only)
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

**Coverage and provenance are not both populated on every field.**
Coverage is meaningful on the *observed/state* side (did we see it);
provenance is meaningful on the *intent/input* side (why does it have
this value). A field materialized from authored intent carries a
provenance and no coverage; a field materialized from an observation
carries a coverage and no provenance. This mirrors `core/nexus/iac`'s own
intent/state split (A0's inventory: `field.go`'s `mode.read`/`write`,
`schema.go`'s "a writable field is a config/intent field... a read-only
field is provider-computed observed state") rather than inventing a new
split.

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
         materialization, not just at release), C5 (applied_defaults/
         derived_values are DERIVED from B3's per-field provenance, not
         independently maintained data).
Open:    C3 (the full v1 field list) and C4 (where the receipt's own
         schema lives) are only PARTIALLY decided -- both have a part
         that depends on sections not yet closed. Does not block C1/
         C2/C5 from being used, but the receipt is not a finished,
         implementable artifact until C3/C4 close.
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
**Decision:** every materialization call produces one — not only a
release-time/signing event. This matches what's already landed and
running in Go: `core/nexus/iac`'s `proof` (A0's inventory) is computed
per `Evaluate()` call — i.e. per runtime conformance check, which is far
more frequent than a release — and `digest.go`/`node.go`'s `Digest()`
functions are plain, cheap, always-available calls with no "only at
release" gate anywhere in the code. Treating the receipt as release-only
would be new, invented behavior inconsistent with what's already proven;
adopting "every call" is the same "don't invent what's already decided
by running code" posture section A took for the canonical format itself.

### C3. Minimum field set — only partially decided

Reconciling `BOUNDARY.md`'s sketch with what Relay's `proof` already
has landed (A0's inventory) surfaces more fields than either source
names alone:

```text
field                      source                  status
---------------------------------------------------------------------
schema.digest              BOUNDARY.md             decided
input_digest                BOUNDARY.md             decided
output_digest                BOUNDARY.md             decided
applied_defaults[]          BOUNDARY.md             decided -- see C5
derived_values[]            BOUNDARY.md             derived -- see C5
signature                   proof.go (A0, Go-only)  decided -- not in
                                                      BOUNDARY.md's
                                                      sketch at all;
                                                      Relay already
                                                      signs this, adopt
engine.version / grammar_version / primitive_release /
validator identity          OPEN-QUESTIONS' own C candidate list
                                                      OPEN -- this is
                                                      section D (version
                                                      binding)'s job, not
                                                      C's; C just
                                                      reserves the field
unresolved/unknown markers  OPEN-QUESTIONS' own C candidate list
                                                      OPEN -- genuinely
                                                      blocked on B2
                                                      (`missing`/
                                                      `unknown`); cannot
                                                      specify the shape
                                                      of a marker for a
                                                      state that isn't
                                                      defined yet
conformance_plan_digest /
observation_digest          proof.go (A0, Go-only)  OPEN -- this is
                                                      section F (output
                                                      symmetry)'s
                                                      territory (intent/
                                                      state comparison),
                                                      not provenance;
                                                      named here so it
                                                      isn't silently
                                                      dropped, not
                                                      claimed as decided
```

So C3 is decided for the data this section itself governs (digests,
signature, the two provenance-derived lists) and explicitly open for the
three rows that are really another section's job to fill in — this
section reserves their place rather than inventing premature shapes for
them.

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

### C5. `applied_defaults`/`derived_values` are derived from B3's per-field provenance, not a second source

**Decision:** the receipt does not independently track which fields were
defaulted or derived — it is **computed from** `MaterializedField.provenance`
(B3), already decided and already present per field. Walking every
materialized field: a `provenance: schema_default` field contributes one
`applied_defaults` entry; a `provenance: derived` field contributes one
`derived_values` entry. This closes `BOUNDARY.md`'s implicit ambiguity
(it shows both the per-field idea and the receipt's aggregate lists
without stating which drives which) in the direction that avoids a
second, independently-maintained source of the same fact — exactly the
two-sources-of-truth risk this whole effort exists to prevent (cf. A0's
own framing: two implementations of the same fact silently diverging).

### C6. What this does and doesn't close

**Closed:** C1 (sibling artifact), C2 (produced every call), C5 (derived
from B3, not duplicated).

**Open:**
- C3's version-identity fields — section D's job, reserved here, not
  specified here.
- C3's unresolved/unknown markers — blocked on B2, cannot be specified
  until `missing`/`unknown` have a real shape.
- C3's `conformance_plan_digest`/`observation_digest` — section F's
  territory (intent/state comparison), named so it isn't dropped, not
  claimed as settled.
- C4's exact schema location — deferred until C3 is actually complete.
