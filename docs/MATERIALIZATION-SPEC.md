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

## B — Semantic state model (CLOSED)

```text
Status: DECIDED, closed. B2 (missing/unknown, and the "is a fourth axis
needed" meta-question) is now resolved -- see B2 below for the
cross-document argument, not a repeated assertion.
Closed:  B1 (the three axes, confirmed final), B2 (missing = coverage's
         absent; unknown = a new, fourth coverage value), B3
         (materialized-type shape), B4 (A0.4's conformance conflict),
         B5 (default_injection's two gates).

Named, not blocking this section's closure (PRIMITIVE-IR.md's
`authored-absent` is not one of BOUNDARY.md's five terms; it came up
only as supporting evidence while closing `missing`): review caught
that an earlier draft wrongly conflated `authored-absent` with an
authored literal `null`. Its exact representation is a separate,
genuinely open question -- see B2's closing paragraphs -- that belongs
with section A's canonical value representation (null-as-a-value vs.
null-as-absence-marker), not with B2's actual scope.
```

**Decision:** the "five distinct statements" `docs/BOUNDARY.md` names
(`missing`, `unknown`, `not_observed`, `not_implemented`, schema-applied
default) are not five values of one enum. They are points on **three
separate, orthogonal axes** — collapsing them onto one axis is exactly
the mistake `BOUNDARY.md` warns against, and keeping them on three means
a field can independently be, say, `not_implemented` (capability) **and**
`unobserved` (coverage) **and** `authored` (provenance) at once, each
fact recorded separately rather than forced into one slot. B2 below
closes the remaining question of exactly where `missing` and `unknown`
land, and confirms three axes is the final count.

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

### B2. Mapping `BOUNDARY.md`'s five terms onto the three axes — CLOSED

```text
BOUNDARY.md term      Axis         Value
----------------------------------------------------------------
not_implemented        capability   not_implemented
(a schema-applied
 default)              provenance   schema_default
not_observed           coverage     unobserved
missing                coverage     absent
unknown                coverage     unknown (new value, decided below)
```

**Closing decision: `missing` ≡ coverage's `absent`.** An earlier draft
asserted this as a recovered fact; review correctly rejected that,
because nothing in `cic-primitives` or `core/nexus/iac` directly defines
`missing` as a synonym for `absent`. Closing it now requires an actual
argument, not a repeat assertion — found by cross-reading this repo's
*other* document against `BOUNDARY.md`, not by re-reading `BOUNDARY.md`
alone:

`docs/PRIMITIVE-IR.md`'s "Complete" property independently enumerates
**three** reasons a value can be absent — not five: *"If a value is
absent, the IR says so explicitly and says why — **authored-absent**,
**not-observed**, **not-implemented** — and these are distinct."* This
is the same repo, the same effort, naming the absence-reasons from
scratch a second time, and it does not reserve a slot for a separate
"missing." Two ways to read that:

1. `PRIMITIVE-IR.md`'s author considered "missing" not worth a fourth
   slot because it is not actually distinct from one of the three — the
   simplest explanation, given no third document independently invents
   a fourth reason either; or
2. `PRIMITIVE-IR.md`'s three reasons are themselves coarser than
   `BOUNDARY.md`'s five terms, with `PRIMITIVE-IR.md`'s single
   "not-observed" standing for the *union* of what `BOUNDARY.md` names
   separately as `not_observed` (never looked) and `missing` (looked,
   confirmed gone) — i.e. exactly `core/nexus/iac`'s own
   `unobserved`/`absent` split, which has existed in landed code the
   whole time and needs no new concept to explain either term.

Both readings converge on the same answer: `missing` does not need a
home outside `coverage`'s existing three values. `absent` already means
*"this observe call looked and the field is authoritatively not
there"* — precisely `BOUNDARY.md`'s own example sentence for the
concept (*"an adapter could not observe `power_state`"* describes
`unobserved`; the field being *confirmed* gone after a real look is what
`absent`/`missing` both describe). Treating them as the same value,
closed, not open.

**Correction (review-caught): `authored-absent` is not settled by this
section, and must not be conflated with an authored literal `null`.**
An earlier draft claimed `authored-absent` is simply `provenance:
authored` with `value: None`, illustrated by "an operator who writes
`field: null`." That conflates two things `PRIMITIVE-IR.md`'s own
wording keeps apart: *"the authoring side explicitly establishes
absence"* is a statement about **presence**, not about **the value
being the literal null**. If a field's schema legitimately accepts
`null` as a real scalar value, then `value: Some(null), provenance:
authored` is an authored *value* (which happens to be null) — a
materially different fact from "the operator declined to provide a
value for this field at all," which is what `authored-absent` is
actually naming. Whether `authored-absent` is represented as
`provenance: authored` + `value: None` (the field has a presence marker
distinct from a null-valued field) is therefore a **representation
decision this section does not make**, not something
`PRIMITIVE-IR.md`'s wording proves on its own. Left open, named
precisely rather than quietly assumed — closing it properly would need
to also settle how an authored literal `null` is distinguished from
authored absence in section A's canonical value representation, which
is out of scope for B2.

Separately, still worth stating: `missing` has a completely different,
already-decided meaning one level up, in `cic-schema-registry`'s
`coverage.py` (this session's earlier D-017 work) — there, `"missing"`
is a **schema-evolution** violation kind: a field declared in a
base/prior schema version with no restatement at all in the derived/new
one, a fact about a *schema's field list across versions*, not about one
*materialized instance's* observed object. Unrelated axes, must not be
conflated — the exact kind of error section A's review caught once
already (`canonicalNumber` vs. `normalizeNumbers`).

**Closing decision: `unknown` is a new, fourth coverage value — decided,
not merely proposed.** Neither `cic-primitives`' `ai/DECISIONS.md` nor
`core/nexus/iac` defines this term; `BOUNDARY.md` names it and never
elaborates, and nothing found while closing `missing` above contradicts
or clarifies it either. In the absence of a counter-reading, the
previously-offered candidate is adopted as decided, since leaving it
open indefinitely blocks section B for a term no further grounding
exists to resolve:

```text
unknown  — this observe call saw the field, and the DEVICE ITSELF
           reported an indeterminate value (e.g. a sensor reporting
           "fault" rather than a reading) -- distinct from `absent`
           (the device affirmatively reports there is no value) and
           from `unobserved` (the observe call never asked).
```

**Closing decision: three axes, not four.** The question `A0`'s
inventory and earlier review raised — whether resolving `missing` would
force a fourth axis (field-value presence/existence, separate from
coverage) — is answered **no**. Both `missing` and `unknown` resolve as
*values within the already-decided coverage axis*, not as a reason to
add a new one. `B1`'s three-axis count is final.

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
    coverage:    observed | absent | unobserved | unknown
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

**Section B is now fully CLOSED.** B2 — the last open item — is
resolved above: `missing` maps to coverage's existing `absent` value
(closed by cross-reading `PRIMITIVE-IR.md`'s independent three-reason
enumeration against `BOUNDARY.md`'s five terms, not by repeating the
earlier, correctly-rejected assertion), `unknown` is adopted as a new,
fourth coverage value, and B1's three-axis count is confirmed final —
no fourth axis.

**Closed:** B1 (the axis model, three axes, final), B2 (`missing`/
`unknown` resolved, see above), B3 (the materialized-type shape, with
`value` correctly optional rather than always-present), B4 (the
resolution of A0.4's conformance conflict as a named lossy projection),
B5 (`default_injection`'s two independent triggers — capability and ACL,
kept separate on both read and write per D-012).

**Resolved downstream, not by this section but recorded here for
continuity:** this section's own earlier draft flagged two open
questions for later sections, both since closed — C5 decided
`applied_defaults`/`derived_values` are computed *from* `provenance`,
not a second source (with C5's own later correction distinguishing
classification from evidence payload); E2/E's `authContextJson` finding
directly answered what prevents a module from reading
`MaterializedField.value` it isn't entitled to (the same host-side gate
B5 already specified, once a real actor identity is threaded through —
itself still an open implementation gap, tracked under E, not B).

**Named while closing B2, not blocking B's closure, not B2's actual
scope:** `PRIMITIVE-IR.md`'s `authored-absent` surfaced only as
supporting evidence for resolving `missing` — it is not one of
`BOUNDARY.md`'s five terms. Review caught that an earlier draft wrongly
equated it with an authored literal `null`; its real representation
(how it's distinguished from a field whose schema-legitimate value
happens to be `null`) is a separate open question, naturally section
A's (canonical value representation), not resolved here.

## C — Receipt schema (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  C1 (sibling artifact, not IR-embedded), C2 (produced every
         materialization, not just at release), C3's version-identity
         row (filled in by D1's four identifier groups, once D closed),
         C5 in full (classification -- provenance alone decides list
         MEMBERSHIP -- AND evidence: DefaultEvidence{rule,
         value_digest} / DerivedEvidence{rule, inputs}, produced
         inline by the same step that applies a default or runs a
         derivation, never folded into B3's closed provenance enum).
Open:    C3's remaining rows (signature; unresolved/unknown markers,
         blocked on F's coverage-projection decision, not B2 or C5;
         conformance_plan_digest/observation_digest, F's territory),
         and C4 (schema location, still deferred until C3's remaining
         rows close). Does not block C1/C2/C3-version/C5 from being
         used, but the receipt is not a finished, implementable
         artifact until C3/C4 close.
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
version-identity fields        D1 (four identifier         decided -- see
(grammar digest,              groups)                     below; D closed
 primitive release,                                       after this row
 schema identity,                                         was first
 validator/engine identity                                reserved. C4
 -- as logical facts,                                     still owns
 NOT a wire layout)                                        nesting/shape
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
unresolved/unknown markers   B1/B2 (coverage axis)        OPEN -- B2
                                                             closed
                                                             (missing ≡
                                                             absent,
                                                             unknown = a
                                                             new coverage
                                                             value), so
                                                             the VALUE
                                                             this row
                                                             would record
                                                             now has a
                                                             concrete
                                                             shape -- but
                                                             WHETHER
                                                             coverage
                                                             projects
                                                             into the
                                                             receipt at
                                                             all is F's
                                                             own,
                                                             still-open
                                                             question
                                                             (F1/F4); not
                                                             decided here,
                                                             not C3's call
                                                             to make
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

**The version-identity row is now CLOSED as to WHICH facts the receipt
must carry, filled in by D's later closure** (it was only a reservation
when this table was first written; D1 has since decided the shape).
**Correction (review-caught): this is not the same as deciding how
those facts are laid out on the wire.** An earlier draft called this
"the receipt's version block," implying a single nested object
(`version: {grammar, primitive_release, schema, engine}`) — a specific
representation D never decided and C4 hasn't either. D1 closed *which
logical identity groups must be present* — grammar digest
(`grammar_sha256`/`grammar_schema_sha256`), primitive release identity
(release tag + `source_commit`/`build_hash`), domain schema identity
(logical identity + version + content digest, D1.3's three-part group),
and validator/engine identity (which implementation produced this
receipt) — not whether they nest under one `version` key, sit as four
flat top-level fields, or something else. That layout question belongs
to C4, same as every other field's placement, and stays open there.

**The `unresolved/unknown markers` row stays open, but its blocker has
moved.** B2's closure gives `unknown` (and `missing` ≡ `absent`) a
concrete value to record — but *whether the receipt records coverage
at all* is explicitly F's own open question (F1/F4: "whether/how
`MaterializedField.coverage` projects into the receipt"), not C3's to
decide. Deciding this row here, now that B2 unblocked the value's
shape, would mean quietly answering F's question from inside C — the
same category of boundary violation this file has caught and corrected
several times already (F1 regressing B3, F3 re-asserting coverage in
the receipt). Left for F.

So C3 is now decided for `BOUNDARY.md`'s own sketch (the two digests,
the two provenance-derived lists) plus D1's four version-identity
facts (content, not layout), and explicitly open for `signature`
(nothing grounds it yet), `unresolved/unknown markers` (blocked on F's
coverage-projection decision, not B2 anymore), and the two F-territory
digests. C4 still owns how every one of these facts is actually nested
and named on the wire.

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

### C5. `provenance` is the classification source; the entry's evidence is a separate, now-specified record — CLOSED

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
work from.

**The execution record, specified — CLOSED.**

**Decision: the evidence is a C-owned companion fact, produced inline by
the same engine step that set the classification, never folded into
`MaterializedField.provenance` itself.** B3's `provenance` enum is
already closed (section B is fully closed) and stays exactly as it is
— a classification, not a payload-carrying variant. This mirrors C1's
own reasoning (the receipt sits *beside* the materialized data, not
inside it, to avoid the archived model's infinite-regress mistake)
applied one level down, at the field's evidence instead of the whole
document's receipt: evidence is a receipt-adjacent fact, not a B3 type
change.

```text
When Normalize applies a schema default to a field:
  it ALREADY knows, at that exact moment, which field and what value it
  substituted. It hands the receipt-builder (same pass, not a separate
  one -- consistent with C2's "every materialization call produces a
  receipt"):

    DefaultEvidence { rule: "schema-default", value_digest: <A7 digest
                       of the substituted value> }

  "schema-default" is a literal, unversioned v1 constant -- the corpus
  shows exactly one default-application algorithm (substitute the
  schema's own declared default), not a family of named rules. If a
  second kind of defaulting is ever needed, it gets its own literal
  name then; nothing here invents a version number for a single rule
  that has never needed one, the same corpus-grounded discipline
  section A applied to the canonical format.

When Resolve/a derivation step computes a field's value from other
fields:
  it already knows which named computation ran and which field paths
  fed it (BOUNDARY.md's own example: "effective-state-v1" from
  admin_state/oper_state). It hands the receipt-builder:

    DerivedEvidence { rule: "effective-state-v1", inputs: [<the exact
                       field paths the rule reads>] }

  Rule identifiers are stable, versioned strings (name + "-v" + number,
  per BOUNDARY.md's own example) -- a rule's logic may evolve under a
  NEW name/version without silently changing what an old receipt's
  entry means, the same versioning discipline D1 applies to grammar
  and schema identity. `inputs` is a STATIC list declared as part of
  the rule's own definition (which paths this named computation always
  reads), not computed dynamically per invocation -- the only corpus
  example (effective_state) has a fixed input set, and a
  data-dependent input list is a real extension but not something any
  evidence requires for v1.

The receipt-builder then projects: walking every materialized field,
`provenance: schema_default` contributes one `applied_defaults` entry
using its `DefaultEvidence`; `provenance: derived` contributes one
`derived_values` entry using its `DerivedEvidence`. No second pass, no
separate bookkeeping -- the evidence was already produced in the same
step that set the classification.
```

This closes the gap the earlier correction named: the execution record
is not a mystery left to "whoever implements `Normalize`" to invent from
nothing — it is the direct, inline output of the exact step that already
has to know this information to do its own job (apply a default,
run a derivation), handed to the receipt-builder in the same pass.

### C6. What this does and doesn't close

**Closed:** C1 (sibling artifact), C2 (produced every call), C3's
version-identity row (filled in by D1's four identifier groups, once D
closed — no new field design, C adopts D's shape), **C5 in full**
(classification — `provenance` alone decides list membership — and
evidence — `DefaultEvidence{rule, value_digest}` / `DerivedEvidence{
rule, inputs}`, produced inline by the same materialization step that
applies a default or runs a derivation, kept as a C-owned companion
fact rather than folded into B3's already-closed `provenance` enum).

**Open:**
- C3's unresolved/unknown markers — B2 closing gave the *value* a real
  shape, but *whether* coverage projects into the receipt at all is
  F's own open question (F1/F4), not C3's to decide. Blocked on F now,
  not B2.
- C3's `conformance_plan_digest`/`observation_digest` — section F's
  territory (intent/state comparison), named so it isn't dropped, not
  claimed as settled.
- C4's exact schema location — deferred until C3 is actually complete
  (still blocked on `signature` and the two F-territory rows above).
- `signature` — nothing grounds it yet; a future decision, not
  recovered from anywhere.

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
3. **Domain schema identity** — not just a version number. **Correction
   (review-caught):** an earlier draft defined this group as "schema
   version" alone (e.g. `storage-resource.v1.2.0`), then D2 went on to
   require "schema identity + version/digest" at the comparability gate
   — a direct contradiction, and for the same reason item 1 isn't a bare
   semver tag: **a version string alone is not an immutable content
   identity.** Two files could both claim `v1.2.0` at different commits.
   This group is three parts, still one logical identifier group (not a
   fifth axis):
   ```text
   schema logical identity / canonical name   (e.g. cic:storage:StorageResource)
   schema version                              (e.g. v1.2.0)
   canonical schema digest                     (content hash, not a tag)
   ```
   All three vary together **per schema**, independently of the grammar
   it's written against — a schema can bump its own version (new field,
   narrowed conformance, ...) without the grammar changing at all, and
   vice versa.
4. **Validator/engine identity** — which *implementation* materialized
   this data, and its own version: e.g. `cic-primitive-engine-rust
   v0.1.0` vs `cic-primitive-engine-go v0.1.0`. Required specifically
   for section G's differential conformance: a receipt has to say which
   side produced it, or two receipts that happen to look alike cannot be
   told apart as "the Go implementation" vs "the Rust implementation,"
   which defeats the entire point of running both. **This is also the
   one identifier that is *supposed* to differ** between the two sides
   under a differential test — a forward note for section G, not solved
   here: "compare the two receipts" cannot mean byte-for-byte identity of
   the *whole* receipt, since this field is defined to disagree by
   design. G will need to separate (a) the materialized *value* bytes
   (section A, expected identical), (b) the receipt's semantic content
   excluding this field (expected equivalent), and (c) this field itself
   (expected to differ) — three different equality questions, not one.

These four fill C3's reserved `grammar_version`/`primitive_release`/
`schema_version`/validator-identity row.

### D2. Go and Rust release independently; full semantic-input identity is the comparability gate

**Correction (review-caught): grammar digest alone is not sufficient.**
An earlier draft gated differential conformance on grammar digest match
alone. That's necessary but not enough: D1 itself establishes that
primitive release identity (D1.2) and schema version (D1.3) vary
*independently* of the grammar digest (D1.1) — so two implementations
could share an identical grammar digest while one materializes against
`primitives/@v0.2.0`'s `storage-resource.v1.2.0` and the other against
`primitives/@v0.2.1`'s `storage-resource.v1.3.0`. Any byte difference
between them would reflect **different semantic input**, not
implementation divergence — exactly backwards from what section G's
differential conformance exists to measure, and in direct conflict with
this file's own target equation for it: *"same authored input + same
schema/primitives version = same canonical materialized bytes + same
receipt semantics."* "Same schema/primitives version" was always part of
that equation; gating on grammar digest alone silently dropped it.

**Decision, corrected:** the Go library and the Rust library do **not**
need to release in version lockstep (one can be at v0.3.0 while the
other is at v0.1.7) — but before section G compares two implementations'
output, **every semantic-input identifier must match, except the one
that is supposed to differ**:

```text
MUST match (comparability gate):
  - grammar_sha256 / grammar_schema_sha256        (D1.1)
  - primitive release identity                     (D1.2)
  - domain schema identity + version/digest         (D1.3)
  - authored input digest                          (section A)

MUST differ, by design -- never part of the gate:
  - validator/engine identity                       (D1.4)

If any MUST-match identifier differs:
  verdict = NOT COMPARABLE
  (never reported as "divergence found" -- the two sides answered
  different questions, so a byte match would prove nothing and a byte
  difference would prove nothing either)
```

This is the concrete mechanism that resolves the risk
`OPEN-QUESTIONS-GO-RUST.md` named for D2 before this section closed: *"so
a mismatched pair is detectable rather than silently producing
'same-looking' but differently-sourced output."* Grammar digest alone
answered a narrower question than that risk actually names.

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
"one or two" framing; independent Go/Rust release, with the full
semantic-input identity (grammar digest + primitive release identity +
schema version/digest + authored input digest) as the comparability
gate — not grammar digest alone, corrected after review caught that an
earlier draft's narrower gate would have let two implementations
materializing genuinely different schema/release versions be compared
as if a byte difference meant divergence (D2).

**Not closed, deliberately:**
- The receipt's exact field layout for these four identifiers (e.g.
  nested under a `version:` block vs. four flat fields) is not fixed
  here — C4 (schema home) still has to produce the actual schema, and
  this section only fixes what the four facts *are*, not their wire
  shape.
- Section G (differential conformance) still has to specify exactly
  *how* a comparability-gate mismatch is reported (a hard failure, a
  skipped comparison with a warning, or something else), and — flagged
  in D1.4 — has to separate three different equality questions
  (materialized-value bytes, receipt semantics excluding engine
  identity, and engine identity itself) rather than treating "compare
  the receipts" as one single byte-equality check. D2 only establishes
  that a gate mismatch must be distinguishable from an actual
  divergence; G has to build the actual comparison logic.

## E — Boundary enforcement (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  E1 (all dispatch paths -- native Go and WASM -- pass through
         the SAME host-side boundary point, mandatorily; neither
         bypasses it -- a fact about WHERE the boundary sits, not what
         happens inside it), E2a (that boundary point must accept only
         a constructor-gated handle, never a raw interface{}), E3 (the
         mechanism is identical regardless of guest language, since it
         runs before any WASM boundary is crossed).
Open:    E2b (the constructor-gated type's exact Go representation and
         the receipt-transport wire shape), whether native and WASM
         modules get the SAME trust/policy treatment inside the
         mandatory boundary (a different question from E1 -- corrected
         after review caught the two being conflated), how the ACL
         actor identity gets threaded through (a real gap found in the
         current code, not invented), and the guest-side
         defense-in-depth digest check's shape.
```

### E1. Enforcement lives at the host, at an already-existing chokepoint — verified against real code, not designed from scratch

**Grounding, not speculation:** `CIC-Relay/core/cabinet/service.go`'s
workflow step executor is *already* the single chokepoint every module
call passes through, regardless of module kind:

```text
3b. inputData, ok := execCtx.Get(step.InputKey)   -- resolved ONCE
3c. validateInputSchema(moduleDesc, inputData)      -- checked ONCE
3d. hashValue(inputData)                            -- hashed ONCE
3e. branch: moduleDesc.NativeImpl != nil  -> native Go call, inputData passed directly
            moduleDesc.WasmCode != nil    -> host.Process(ctx, authContextJson, json.Marshal(inputData))
```

**Verified, not assumed, what happens at 3c/3d today:**
- `validateInputSchema` (`schema_validate.go`) checks only that
  `inputData` is a `map[string]interface{}` carrying a `"$schema"` key
  whose value is in the module's declared `Accepts` list. No field-level
  resolution, no short/long-form expansion, no capability/coverage/
  provenance tagging — this is a routing guard ("does this claim to be
  the right schema"), not materialization.
- `hashValue` (`proof_trace.go`) is `canonicaljson.ToJSON(v)` → SHA-256 →
  hex — **the exact canonical-digest pipeline section A adopted**, already
  in production use here. But it feeds **ProofTrace's own chain-of-custody**
  (per-step input/output hashes, chained into `ComputeChainHashV1`) — a
  *different* proof artifact from this effort's materialization receipt
  (section C). They must not be conflated just because they share the
  same digest machinery: ProofTrace proves *which steps ran with which
  I/O*; the materialization receipt proves *how a field's value was
  derived*. Reusing the canonical-digest code is fine and expected;
  treating the two proof artifacts as one thing is not.

**Decision:** enforcement belongs at this exact chokepoint — between
3b (input resolved) and 3e (dispatch) — not left to each module's own
discretion, and not duplicated once per dispatch branch.

**Correction (review-caught): "one gate covers both paths" conflated two
different claims.** What's actually decided is narrower and purely
structural: **both dispatch branches pass through the identical
host-side boundary point, mandatorily — neither gets a bypass around
it.** That is a fact about *where* the boundary sits, not about what
happens *inside* it. Whether `NativeImpl` and `WasmCode` receive the
*same trust/policy treatment* once past that mandatory point (same
capability/ACL checks, same strictness) is a **separate, still-open**
question — see E4. This section does not say the two paths are treated
identically; it says neither path may route around the boundary itself.
If `NativeImpl` modules ever get a different trust tier, that
differentiation happens *at* this chokepoint, as a policy decision
applied there — it can never mean skipping the chokepoint, or the "every
module call" framing this whole section rests on stops being true.

This answers E1: the host enforces, not the guest, and not "whichever
the module author remembers to call" — consistent with `BOUNDARY.md`'s
own admission that the Rust-side type guarantee doesn't survive
serialization anyway, so a voluntary per-guest convention would be
exactly the kind of unenforced promise that guarantee was never going to
deliver.

### E2. The concrete mechanism, split into what's decided and what isn't

**Correction (review-caught): an earlier draft marked this whole section
decided in one place and open in another** — the same self-contradiction
already fixed once in section B (there, `missing`/`unknown`; here, this
item). Splitting it into the architectural requirement (decided) and the
implementation shape (not):

**E2a — CLOSED.** The host dispatch loop (`service.go`'s step 3e) MUST
NOT accept a raw `inputData interface{}` for either branch. It may only
accept a handle that can only have been produced by calling the
materialization library — a constructor-gated type, private fields,
no literal-construction path, mirroring `BOUNDARY.md`'s
`Materialized<T>`/`Validated<T>` pattern (originally specified for Rust)
applied to **this Go host code specifically**, where it is achievable:
host and resolver run in the *same process* here, at step 3b, before
anything crosses a WASM boundary — the "in-process" case `BOUNDARY.md`
already said was achievable, just not yet built. (Not the same claim as
node.go's `Node` — A0/B3 already corrected that overclaim; this is new
work, not a reuse of something already landed.) This is the
architectural decision; it does not depend on exactly how the type is
shaped.

**E2b — OPEN.** The type's exact Go representation: field layout,
constructor signature, which accessor methods exist (canonical JSON?
receipt? both?), and the wire shape for carrying the receipt alongside
the bytes once they do cross a boundary (`service.go`'s current
`host.Process(ctx, authContextJson, inputJson string)` only has room for
two strings — a receipt-carrying envelope isn't designed here, flagged
as a real follow-up for whoever implements this, not assumed solved).
None of this is fixed by E2a; only that *some* constructor-gated type
must exist and sit at that exact chokepoint.

**A real gap, found by reading the code, not hypothesized:**
`authContextJson := step.ComponentID` (`service.go`) is **not a genuine
actor identity** — it's the calling component's own ID string, not
something `core/nexus/iac/acl.go`'s `ACL.Allows(actor, ...)` (A0.1) could
evaluate meaningfully. For B5's capability/ACL gates
(`default_injection` substitution) to run as part of this same
pre-dispatch step — which is where they need to run, so a module is
simply never handed a value it isn't entitled to, rather than trusted to
self-filter — `authContextJson` needs to carry a real identity. **This
section does not resolve that** — whether Relay's existing auth/identity
system already has one available at this call site, or needs one built,
wasn't checked; named as an open, concrete gap rather than assumed either
way.

This directly answers the question B5 left open (*"what prevents a
module from reading `MaterializedField.value` directly, bypassing B5's
response-time gates, for a module that is itself an untrusted
requester"*): the answer is this same host-side gate, at this exact
point in `service.go` — not a separate mechanism.

### E3. Same mechanism regardless of guest language

The gate above runs entirely in `service.go`, in Go, before branching to
either `NativeImpl` or `WasmCode` — and before any WASM boundary is
crossed at all. Whether the eventual WASM guest was compiled from Go or
Rust source is invisible at this call site; by the time
`host.Process(ctx, authContextJson, inputJson)` runs, it's bytes either
way. So E1/E2's host-side enforcement is identical for a Go guest and a
Rust guest — answering E3.

The one place language *does* matter is defense-in-depth: a guest
independently verifying `digest(received_bytes) == receipt.output_digest`
(section A7's digest format) on its own side, catching a corrupted
transport or a host bug, not relying solely on the host's gate. This is
cheap (a hash-and-compare, not a re-materialization) and needs
implementing once per guest language — the same two-implementation-parity
concern as everything else in this effort, not designed further here.

### E4. What this does and doesn't close

**Closed:**
- **E1** — all dispatch paths (`NativeImpl` and `WasmCode`) pass through
  the same host-side boundary point (`service.go`'s identified
  chokepoint), mandatorily; neither gets a bypass around it. This is a
  fact about *where* the boundary sits, not about what happens inside it
  (see open item below).
- **E2a** — that boundary point must accept only a constructor-gated
  handle, never a raw `inputData interface{}`.
- **E3** — the mechanism is language-independent, since it runs before
  any WASM boundary is crossed.

**Not closed, deliberately:**
- **E2b** — the constructor-gated type's exact Go representation (field
  layout, constructor signature, accessor methods) and the wire shape
  for carrying a receipt alongside bytes once they cross a boundary
  (`service.go`'s current `Process(ctx, authContextJson, inputJson
  string)` only has room for two strings — a receipt-carrying envelope
  needs designing, not assumed to already fit).
- **Trust/policy treatment inside the mandatory boundary** — whether
  `NativeImpl` (first-party, non-WASM Go modules compiled into Relay
  itself) and `WasmCode` get the *same* capability/ACL checks once past
  the chokepoint, or `NativeImpl` earns a different trust tier there.
  Explicitly a different question from E1 (which only fixes that neither
  path bypasses the chokepoint itself) — not decided either way.
- How `authContextJson` becomes a real actor identity — a genuine,
  found-not-invented gap, left for whoever owns Relay's auth/identity
  binding.
- The guest-side digest-verification SDK shape (Go and Rust) — not
  designed here, just established as necessary.

## F — Output symmetry (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  F1-model (B's coverage/provenance axes and A's canonical form
         already cover output, with NO input-vs-output split -- the
         real split is observation-knowledge vs. value-origin, and a
         derived state field legitimately carries both at once, per
         B3), F1-mechanism (C2's every-call receipt production is
         direction-agnostic), F2 (core/nexus/iac's compare.go/
         observation.go/conformance.go are migration source per A0's
         existing meta-decision, not a permanently separate contract --
         made explicit for the output axis, not newly decided).
Open:    whether/how MaterializedField.coverage gets PROJECTED INTO
         the receipt for an observed field -- C3's decided field set
         (digests + applied_defaults[]/derived_values[]) carries no
         coverage payload today, and conformance_plan_digest/
         observation_digest are explicitly still reserved for this
         section, unresolved. Also open: the comparator/verdict
         EXECUTION LOGIC (doesn't exist in the new lib yet, in either
         language), and how the conformance/drift verdict relates to
         the receipt (C) and to ProofTrace (E's finding) -- three
         adjacent, distinct proof artifacts, not one.
```

### F1. The model already covers output; the executable logic doesn't exist yet

**Recovered, not newly decided:** `docs/BOUNDARY.md`'s own diagram
already specifies the output path's shape —

```text
ValidatedMaterializedInput -> module -> UntrustedModuleOutput
                                      -> output schema validation
                                      -> ValidatedObservation / ValidatedConsequence
```

— and states the principle plainly: *"A module's output does not
inherit trust from its input. An adapter receives a proven contract,
talks to a real system, and returns a raw observation; that observation
is validated against the state/operational schema before it may enter
the CIC state or proof chain."* This was never actually in question;
what section F has to establish is how *this effort's* machinery (A-E)
applies to it.

**It already does, without new scope — corrected after review caught a
regression to B3's pre-correction framing:**
- **Section B's model is already symmetric, but not along an
  input-vs-output split.** An earlier draft of this section said
  "`coverage` for output, `provenance` for input — never both," which is
  exactly the oversimplification B3 was already corrected away from: a
  *derived state field* carries **both** `coverage: observed` and
  `provenance: derived` at once (`BOUNDARY.md`'s own `effective_state`
  example). The real split is not "which direction" but **what each axis
  records**: `coverage` is *observation knowledge* (was this field
  looked at, and what did the look find), `provenance` is *value origin*
  (authored, defaulted, or derived) — and they cross:
  ```text
  raw observation       -> coverage populated, provenance usually absent
  derived state field    -> coverage populated, provenance: derived
  authored intent field  -> provenance populated, coverage absent
  derived intent field   -> provenance: derived, coverage absent
  ```
  There is no second data model to invent for output; B already built
  one model that covers both, correctly, as of the fix already merged —
  this section's first draft just re-introduced the bug in its own
  summary of that fix.
- **Section A's canonical form has no input/output distinction at all** —
  a canonical value is a canonical value, materialized from an
  observation or from authored intent, byte-identical rules either way.
- **Section C's receipt mechanism is direction-agnostic, but this does
  NOT mean the receipt carries a coverage payload — corrected, a second
  overclaim in the same paragraph.** C2 decided a receipt is produced on
  *every materialization call*; nothing in C1-C5 says "input only," so
  the *mechanism* is symmetric. But C3's actually-decided field set is
  digests plus `applied_defaults[]`/`derived_values[]` (both derived from
  `provenance`, per C5) — **it does not define any coverage/observation
  field at all**, and C3 explicitly reserved
  `conformance_plan_digest`/`observation_digest` as *this section's*
  territory, still open. So: `MaterializedField.coverage` (B3) is
  in-memory state, already decided. Whether — and how — that coverage
  information gets *projected into the receipt* for an observed field is
  a **separate, still-open question**, not something this section
  (or C) has settled.

**What doesn't exist yet, in either language:** the actual comparator
and verdict-aggregation *logic* — deciding whether an observed value
conforms to a declared intent, and rolling per-field verdicts up to an
object verdict. Today this exists only as Relay's own Go code
(`core/nexus/iac/compare.go`'s `CompareType` exact/numeric comparator,
`observation.go`'s `ClassifyField`, `conformance.go`'s `Evaluate`/
`aggregate`) — landed, tested, but scoped to one OCI vertical slice and
with **no Rust peer at all**. This is the same "model decided, Rust
implementation doesn't exist yet" shape section A was already honest
about for canonicalization; F doesn't change that shape, it just
confirms the comparator is squarely inside it.

### F2. `core/nexus/iac`'s comparator is migration source, not a permanent fork — making A0's existing decision explicit for this axis

A0's meta-decision already settled the general question: *"the
materialization library is the single semantic authority... `core/
nexus/iac` is migration source and tested reference material, not a
second, competing contract to keep alive indefinitely."* This section
does not re-decide that — it states plainly that the comparator
specifically falls under it, because `OPEN-QUESTIONS-GO-RUST.md`'s
original F2 asked this as if it were still open, and leaving an
already-decided principle looking undecided for one specific piece of
code is its own kind of drift risk.

So: `compare.go`/`observation.go`/`conformance.go` do not stay separate
from the new lib indefinitely. They are the inventory for the output
side exactly as the rest of `core/nexus/iac` was inventory for the input
side (A0.1) — eventually absorbed per the roadmap's step 6 (migrate
Relay onto the library), not maintained forever as a second
implementation of the same judgment. Until that migration, Relay's
existing OCI vertical slice continues to run on its own code unchanged;
nothing here requires touching it now.

### F3. Three distinct proof-adjacent artifacts — not one, and not two

Building on the exact distinction E1 already drew between the
materialization receipt (section C) and ProofTrace's chain-of-custody:
there are now three artifacts in view, and they must stay distinct:

```text
1. ProofTrace chain       -- which workflow steps ran, with which
   (service.go/              input/output hashes, chained
    proof_trace.go)           (execution audit, not field semantics)
2. Materialization receipt -- materialization evidence bound to the
   (section C)                canonical result: today, digests plus
                               provenance-derived default/derivation
                               evidence (C3/C5). Whether coverage/
                               observation evidence ever becomes part
                               of this artifact is OPEN, not decided
                               by this section (see F1/F4) -- not
                               restated as settled here.
3. Conformance/drift verdict -- intent vs. observed comparison outcome
   (conformance.go)            (CONFORMANT/DRIFT/OBSERVED_ABSENT/
                                UNOBSERVED/NOT_COMPARABLE)
```

**Correction (review-caught, a third occurrence of the same error F1/F4
already fixed twice in this section):** this subsection's own prose
originally said the receipt carries "capability/coverage/provenance
evidence" and "says how its value came to be (defaulted, derived,
**observed**)" — both quietly re-asserting the coverage-in-receipt claim
F1/F4 explicitly mark OPEN, in the same document, two headings later.
Fixed to describe only what C actually decided (digests +
provenance-derived evidence), with the coverage question named as open
rather than answered a third time by implication.

`C3` already flagged `conformance_plan_digest`/`observation_digest` as
"section F's territory, not provenance" — this section confirms why:
artifact 3 is not a part of artifact 2, whatever artifact 2 eventually
includes. The conformance verdict says whether an observed value matches
what was declared; the receipt says what C already specifies today
(digests, provenance-derived evidence) — not a claim about coverage.
Related, sequential, and currently computed by overlapping code paths in
Relay — but not the same fact, and not to be merged into one structure
for convenience.

### F4. What this does and doesn't close

**Closed:** F1's model claim (B's existing coverage/provenance axes
already cover output, with no input-vs-output split — corrected after
review caught a regression to B3's pre-correction framing), F1's
mechanism claim (C2's receipt production is direction-agnostic), F2
(the comparator is migration source under A0's existing meta-decision,
not a permanent fork — explicit, not newly decided), and the
three-artifact distinction (F3).

**Not closed, deliberately:**
- **Whether/how `MaterializedField.coverage` gets projected into the
  receipt** — corrected, a second overclaim an earlier draft made in
  the same paragraph as the one above. The receipt mechanism being
  direction-agnostic (C2) does not mean C3's actual field set carries a
  coverage payload — it doesn't, today, and `conformance_plan_digest`/
  `observation_digest` remain exactly as open as C3 already said they
  were.
- The comparator/verdict logic has no implementation in the new lib at
  all yet, in either language — this section establishes where it
  belongs, not its code.
- How artifact 2 (receipt) and artifact 3 (conformance verdict) relate
  procedurally — e.g. does computing a verdict require a receipt to
  already exist, or are they independent outputs of the same
  materialization call — is not decided here.
- Whether/how `conformance_plan_digest` (sketched, not landed, per the
  A0/D correction) ever gets built is not this section's job.

## G — Differential conformance (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed -- and cannot fully close until
C3's open fields and F's comparator implementation do (B2 and C5 have
since closed in later passes and no longer belong on this list). This
section fixes the comparison HARNESS's structure and two scoping
questions; it does not, and cannot yet, specify full test coverage.

Closed:  G1 (the existing conformance/ corpus is extended, not
         duplicated), G2 (check_grammar.py stays out -- confirmed
         deliberate, not a gap), the harness structure (G3): gate
         first using D2's comparability check, then compare canonical
         bytes (A), then compare receipt semantics excluding engine
         identity (D1.4's forward note) -- three separate questions,
         never one byte-equality check on everything.
Open:    full coverage. Anything touching C3's still-open fields (its
         unresolved/unknown markers row specifically -- B2 itself has
         closed, but that row's receipt-field shape has not) or F's
         not-yet-implemented comparator is explicitly out of the
         harness's scope until those sections close -- not silently
         skipped. C5's evidence record is no longer on this list: it
         closed with a specified shape (DefaultEvidence/DerivedEvidence),
         so it is not a harness-scope blocker any more.
```

### G1. Extend the existing corpus; don't build a parallel one

**Decision:** `conformance/` already holds a language-independent
vector corpus (`input.yaml`/`expected.yaml` pairs, today one group:
`reader/`) with a harness that enforces it can't trivially pass (empty
corpus fails; a group with no accepted vector fails) — `engine/tests/
conformance.rs`, verified directly, not assumed. This effort adds a new
group (e.g. `conformance/materialization/`) to the same corpus,
governed by the same harness properties, rather than building a second,
parallel vector mechanism. The corpus's own README already states the
reason to do this, not invent a new one: *"a corpus proves what it
contains... differential execution finds what nobody thought to write
down"* — one corpus, one harness, extended, not duplicated.

**Precision, not assumed: adding a group does not make execution
automatic.** `engine/tests/conformance.rs` has two distinct parts, read
directly: a *generic* test that walks every group and checks corpus
invariants only (non-empty, minimum vector count, at least one
accepted) — group-agnostic, and already free for a new
`materialization/` group — and `reader_vectors()`, a *separate*, named
test hardcoded to the `reader` group specifically, which actually runs
the engine against each vector and asserts the expected outcome. A new
`materialization/` group gets the generic invariant checks immediately;
an analogous `materialization_vectors()` test, actually exercising the
resolver, still has to be written by hand, same as `reader_vectors()`
was. Noted so this isn't later assumed to already be generic.

### G2. `cic-primitives`' grammar checker stays out — confirmed deliberate, not re-decided

**Recovered, not newly decided:** sections A0 and F already established
that `check_grammar.py` validates **static schema structure** (is
`role: config` legal, are structural positions closed correctly) — an
authoring-time concern — while this effort's materialization lib
resolves **value instances** at runtime. These are different axes, not
competing implementations of the same fact, so there is nothing for
`check_grammar.py` to differentially agree or disagree with the Go/Rust
materialization pair about. It does not join as a third oracle. Stated
here explicitly so the split reads as a decision, not an omission.

### G3. The comparison harness: three separate questions, not one

Per D1.4's own forward note (written when D1 introduced the one
identifier — validator/engine identity — that's supposed to differ):
*"'compare the receipts' cannot mean byte-for-byte identity of the whole
receipt... G will need to separate (a) the materialized value bytes, (b)
the receipt's semantic content excluding this field, and (c) this field
itself — three different equality questions, not one."* This section
fixes that structure:

```text
For each conformance vector, given a Go-side and a Rust-side result:

1. COMPARABILITY GATE (section D2) -- checked first, always:
   grammar digest, primitive release identity, schema identity/
   version/digest, and authored input digest MUST match between the
   two sides. Validator/engine identity MUST differ (D1.4) and is
   never part of this check.
     -> mismatch on a MUST-match identifier: verdict = NOT_COMPARABLE.
        Stop here. This is not a divergence -- the two sides answered
        different questions.

2. VALUE EQUALITY (section A) -- only reached if gated above passed:
   canonical materialized bytes, compared directly.
     -> mismatch: verdict = DIVERGENCE.

3. RECEIPT SEMANTIC EQUALITY (sections C, D1.4) -- only reached if (2)
   matched. **Correction (review-caught): this is a semantic
   projection, not a wire-level field removal** -- C4 (receipt schema
   home/exact layout) is still open, so there is no fixed field path
   to name "remove" yet; specifying one here would decide C4's layout
   by accident, from inside G, before C4 itself closes.

   Construct the comparison projection of each receipt:
     - include every receipt field whose semantics are required to
       agree between implementations;
     - exclude validator/engine identity (D1.4), which is intentionally
       implementation-specific and must never be compared for equality.
   Canonicalize each projection (section A's format, reused -- not a
   new algorithm) and compare the resulting bytes.
     -> mismatch: verdict = DIVERGENCE.

   Once C4 fixes the receipt's actual field layout, this projection's
   mechanical definition (which fields it includes) follows
   automatically -- G does not need revisiting, only applying.

Neither step 2 nor step 3 is reached for a vector that exercises a
field whose semantics are not yet decided (C3's still-open fields --
B2's `missing`/`unknown` have since closed, but C3's receipt-field row
for them hasn't; C5's evidence record has since closed with a
specified shape and is no longer in this category) -- such a vector is
OUT OF SCOPE for this harness today, named as a gap, not silently
treated as passing or skipped without record.
```

This gives differential conformance its own verdict vocabulary
(`NOT_COMPARABLE` / `DIVERGENCE` / implicit match when neither fires) —
**deliberately not** `conformance.go`'s `CONFORMANT`/`DRIFT`/
`OBSERVED_ABSENT`/`UNOBSERVED`/`NOT_COMPARABLE` vocabulary (F3, artifact
3), which answers a different question (does an *observed* value match
a *declared intent*) than this one does (do *two implementations*
produce the same materialization for the *same* input). Sharing the
term `NOT_COMPARABLE` across both is a coincidence of English, not a
shared concept — named here before it causes the same kind of
conflation E1 and F3 already had to correct twice.

### G4. What this does and doesn't close

**Closed:** G1 (corpus extended, not duplicated), G2 (grammar checker
confirmed out of scope, deliberately), G3 (the three-question harness
structure, reusing D2's gate and A's canonical comparison rather than
inventing new mechanisms).

**Not closed, and cannot be yet:**
- Full test coverage — blocked on C3's still-open fields (its
  `unresolved/unknown markers` row specifically; B2 itself closed in a
  later pass) and F's comparator/verdict implementation (doesn't exist
  in the new lib in either language). C5's evidence-record shape has
  since closed and no longer blocks this. The harness structure is
  ready to run the moment the remaining items close; it cannot run
  completely before they do.
- The actual Go and Rust implementations this harness would exercise —
  this document specifies what they must agree on, not their code.
- The `materialization_vectors()` execution function itself — per G1's
  precision note, adding a corpus group only gets the generic
  invariant checks; the hand-written test that actually runs the
  resolver against each vector (mirroring `reader_vectors()`) still has
  to be written.
- Whether `NOT_COMPARABLE`/`DIVERGENCE` need richer sub-classification
  (e.g. which specific identifier mismatched) is left to whoever
  implements the harness, not fixed here.

**This closes the first pass through A-G.** Every section now has at
least a decided core; B is fully closed (as of a later pass), C, E and
F remain explicitly partial, each with named, specific open items
rather than an unexamined "TBD." The next work is closing those named
items — C3's reservations, E2b, F's comparator — not starting new
sections.
