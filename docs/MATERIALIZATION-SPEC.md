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
for them. This answers `PRIMITIVE-IR.md`'s open question #3 (*"what is
the canonical form — a YAML profile, canonical JSON, or something the
engine defines outright?"*): canonical JSON, specifically this
already-landed encoding, not a new format.

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
- **Update, no longer accurate as first written: the Rust side now
  implements all of A1–A7.** This bullet originally said the Rust side
  implemented none of A2/A4/A6/A7, with only `cic-canonical` (the
  separate `CIC-Relay` crate) covering A3. `engine/src/canonical.rs`
  (this repo's own, self-contained module — PR #19) now implements A2
  (key ordering), A3 (numbers — `cic-canonical`'s proven logic ported
  by hand, not taken as a cross-repo dependency), A4 (string escaping,
  re-verified empirically against Go's `encoding/json` in a Go
  container, not assumed from this document's own table), A6 (array
  order) and A7 (digest), with unit tests mirroring
  `canonicaljson_test.go`'s and `cic-canonical`'s own vectors, plus
  differential vectors diffed byte-for-byte against real Go output at
  the extremes (`1e-100`, `5e-324`, integers beyond `i64`). A review on
  that PR also found and closed a real gap this document's own A3 text
  had not yet caught in code: `saphyr` (this engine's YAML parser)
  silently demoted an integer literal beyond `i64` range to a lossy
  `f64` before canonicalization ever saw it — fixed in `reader.rs`,
  not papered over here, via a new `Value::BigInt(String)` carrying
  the literal's exact digits. Not yet wired into a pipeline stage —
  `Parse`/`Normalize`/`Resolve`/`Validate`, which would produce a
  materialized tree to canonicalize, still don't exist — so this is a
  standalone primitive, correct and tested in isolation, not yet
  exercised end-to-end through a real composition.
- Section B (semantic state model) still has to decide how the five-state
  concern and A0.4's tri-state/boolean conformance conflict interact with
  this canonical form — e.g. whether a `not_implemented`/`deprecated`
  marker is itself subject to A2's key-ordering once it's part of the
  materialized tree. Not decided here. **Closed later, as B7** (added
  after F1 surfaced that this forward reference was never actually
  answered when B first closed): capability/coverage/provenance ARE part
  of the materialized semantic claim `output_digest` commits to — but B7
  decides that as semantic membership, not as the wire layout A2 governs;
  the exact nesting/ordering question this bullet poses stays open, for
  C4/E2b.

## B — Semantic state model (CLOSED)

```text
Status: DECIDED, closed. B2 (missing/unknown, and the "is a fourth axis
needed" meta-question) is now resolved -- see B2 below for the
cross-document argument, not a repeated assertion. B7 is a later-pass
addition (added while closing F1): A8 had deferred a question to this
section that B's original closure never actually answered -- whether
capability/coverage/provenance are part of the materialized semantic
output at all. B7 answers it: yes, as semantic membership/canonical
commitment, not as a wire-layout decision.
Closed:  B1 (the three axes, confirmed final), B2 (missing = coverage's
         absent; unknown = a new, fourth coverage value), B3
         (materialized-type shape), B4 (A0.4's conformance conflict),
         B5 (default_injection's two gates), B7 (capability/coverage/
         provenance are part of the materialized semantic claim
         output_digest commits to -- closing A8's forward reference).

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

### B7. The three axes are part of the materialized semantic claim — closing A8's forward reference

**This closes a gap A8 named but B's original closure never actually
answered.** A8 explicitly deferred: *"Section B... still has to decide
how the five-state concern... interact[s] with this canonical form —
e.g. whether a `not_implemented`/`deprecated` marker is itself subject
to A2's key-ordering once it's part of the materialized tree."* B6's
closure never revisited this — a real, previously untracked gap,
surfaced only while attempting to close F1/F4 (section F cannot decide
whether `coverage` needs *projecting into the receipt* until it's
settled whether `coverage` is part of the *materialized output* at
all in the first place).

**Decision: capability, coverage, and provenance are part of the
materialized semantic output this engine commits to — not purely
in-memory/engine-internal facts that vanish at the module boundary.**

Two already-closed decisions force this, taken together, not a new
assumption:
- **B3** already states what a module actually sees: *"the value and
  its status are different things a module can inspect independently,
  and a module reading `.Value` never has to parse status out of the
  value's own shape."* A module can inspect capability/coverage/
  provenance, not only the bare value.
- **`BOUNDARY.md`** already states what happens to in-process types at
  a module boundary: *"The type system does not survive
  serialization... what crosses is bytes, and bytes carry no
  `Validated<_>`."* Across any real process boundary (WASM, subprocess,
  wire), only data survives — never a Rust/Go type's internal
  structure.

A module on the other side of a real process boundary can only
exercise B3's promised ability to inspect status independently of
value if that status is actual *data* that crossed the wire — a
type-level annotation that does not survive serialization is not
something a WASM module can "inspect." B3's own promise, combined with
`BOUNDARY.md`'s own rule about what survives a process boundary,
already requires capability/coverage/provenance to be real, serialized
data. This is not this section inventing new scope; it is the
conclusion the two already-closed decisions jointly force, simply
never stated as a canonicalization/commitment decision until now.

**Consequence: a change in any of capability, coverage or provenance is
a change in the materialized claim, and must therefore be covered by
the canonical representation/`output_digest` (A7).** Two
materializations with the identical `value` but different `coverage`
(`observed` vs. `unknown`) or different `provenance` (`authored` vs.
`schema_default`) are not the same materialized claim, and must not
digest to the same `output_digest`. Concretely, for differential
conformance (G): a Go implementation that reports `observed` for a
field and a Rust implementation that reports `unknown` for the
identical `value` must **diverge** at `output_digest` comparison —
section A's canonical-byte-equality step (G3) already catches this
once this decision is in effect, without G needing any new machinery
of its own.

**This decision does NOT prescribe the wire layout.** It closes
*semantic membership* (these three facts are part of what the engine
commits to) and *canonical commitment* (`output_digest` must cover
them) — not *how* they are nested, named, or positioned in the
canonical byte tree next to `value`. That stays exactly where A8
already left wire-layout questions, and where C4/E2b's own open layout
questions already live. Deciding the physical shape here would repeat
the identical boundary violation this document has already corrected
several times (most recently, D1's version-identity facts vs. C4's
wire nesting of them) — this section closes **that this is committed**,
not **how it is written down**.

**Consequence for F1/F4 and C3, named here, closed there:** this does
not mean the *receipt* (section C) must also carry a coverage
projection — coverage no longer needs "rescuing" by the receipt for
custody purposes, because it is already part of the materialized
output `output_digest` protects. Whether the receipt *additionally*
carries a redundant coverage projection as an audit convenience is a
separate, non-custody question, addressed where F1/F4 actually live.

## C — Receipt schema (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed.
Closed:  C1 (sibling artifact, not IR-embedded), C2 (produced every
         materialization, not just at release), C3's version-identity
         row (filled in by D1's four identifier groups, once D closed),
         C5 in full (classification -- provenance alone decides list
         MEMBERSHIP -- AND evidence: DefaultEvidence{rule,
         value_digest} / DerivedEvidence{rule, inputs, value_digest},
         produced inline by the same step that applies a default or
         runs a derivation, never folded into B3's closed provenance
         enum; inputs is what THIS invocation actually read, not a
         static rule-definition property), C3's `signature` row AS
         TO WHETHER THIS ENGINE SIGNS (CLOSED -- it does not, and does
         not decide signer authority; this engine's own README
         excludes Vault access/counter-signature policy from its
         scope), and C3's `unresolved/unknown markers` row (CLOSED --
         B7 settles that coverage, including `unknown`, is part of the
         materialized semantic output committed by `output_digest`; it
         does not need its own receipt row for custody purposes, so
         this row needs no field of its own in C's schema), C4's
         WHICH-REPO question (CLOSED -- the schema lives in this repo,
         `cic-primitive-engine`, not a new or separate location; see
         C4 for the three convergent reasons), and C3's
         `conformance_plan_digest`/`observation_digest` rows (CLOSED,
         by F5 -- confirmed NOT receipt fields at all; they belong to
         the conformance/drift verdict artifact (F3), with their own
         scope and byte-level semantics decided there. This closes the
         row for C's purposes without implementing anything -- F1's
         comparator still doesn't exist in either language), and C4's
         CORE FIELD LAYOUT (CLOSED -- docs/RECEIPT-SCHEMA.md, every
         field cited to the decision requiring it, version-identity
         facts laid out flat per D1 rather than nested, a latent
         overlap between the old `schema.digest` row and D1.3's richer
         group found and resolved -- one field, not two -- AND
         requiredness/cardinality/ordering fixed on review: every
         field REQUIRED, `applied_defaults`/`derived_values` REQUIRED-
         but-possibly-`[]`, both sorted by `.path` byte-wise,
         `derived_values[].inputs[]` a deduplicated sorted set).
Open:    C4's EXTENSION/META fields, not the core layout above: WHETHER
         the receipt schema itself ever carries a
         signature-related field populated by an external authority,
         same pattern as cic-primitives' own release.sign/pledge.sign
         -- C4's call, a schema-layout question, not decided by this
         engine-doesn't-sign closure; smaller, whether the receipt
         should ADDITIONALLY carry a redundant coverage projection as an
         audit convenience, not a custody requirement -- F1/F4's
         territory; and a new, small question found while writing
         RECEIPT-SCHEMA.md: whether each receipt INSTANCE needs its own
         schema-version field, distinct from this document having one.
         Does not block C1/C2/C3-version/C5/C3-signing/C3-unresolved-
         unknown/C3-F-territory/C4-repo/C4-schema-text from being used,
         but the receipt is not a fully closed artifact until the
         signature-slot and coverage-projection questions resolve.
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
schema.digest               BOUNDARY.md                  ABSORBED into
                                                             the version-
                                                             identity row
                                                             below (D1.3's
                                                             `schema_digest`)
                                                             -- found while
                                                             writing
                                                             RECEIPT-
                                                             SCHEMA.md, not
                                                             a separate
                                                             field
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
signature                    this engine's own README      CLOSED, NARROWER
                              ("what does not belong        than first drafted
                              here": Vault access,          -- engine never
                              counter-signature policy)     signs/decides
                              + cic-primitives D-015        signer authority;
                              (release.sign IS a            whether the
                              schema field, populated       schema ever
                              by an external                reserves a slot
                              _vault_sign() call, not       for an
                              the compiler's own build      externally-
                              logic -- schema ownership     produced
                              != producer ownership)        signature is
                                                             C4's open
                                                             question, not
                                                             decided here;
                                                             see prose below
unresolved/unknown markers   B1/B2 (coverage axis) +     CLOSED -- no
                              B7 (coverage is part of     receipt row
                              the materialized semantic   needed; B7
                              claim, committed by          commits
                              output_digest)               coverage via
                                                             output_digest
                                                             already, the
                                                             same
                                                             mechanism
                                                             protecting
                                                             `value`;
                                                             whether a
                                                             REDUNDANT
                                                             audit-
                                                             convenience
                                                             projection is
                                                             ALSO added is
                                                             a separate,
                                                             smaller,
                                                             non-blocking
                                                             question
                                                             (F1/F4)
conformance_plan_digest      F5 (this document) --         CLOSED --
                              no landed precedent           NOT a
                              (`ConformancePlan` exists     receipt
                              in `conformance.go` but is    field; see
                              never passed to a digest      F5 for the
                              function there)               verdict-
                                                             artifact
                                                             scope and
                                                             byte-level
                                                             semantics
observation_digest           F5 (this document) --         CLOSED --
                              widens Relay's landed         NOT a
                              `observationDigest()`, which  receipt
                              digests only the coverage     field; see
                              envelope (`Observation{        F5 for the
                              Observed, AuthoritativeAbsent}`) verdict-
                              -- never the observed values  artifact
                              map -- a narrower scope than   scope and
                              this name suggests             byte-level
                                                             semantics
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

**The `unresolved/unknown markers` row is now CLOSED, by B7 — added
later, while closing F1.** B2's closure gave `unknown` (and `missing` ≡
`absent`) a concrete value to record, but *whether the receipt records
coverage at all* stayed F's own open question (F1/F4) rather than C3's
to decide — correctly left there at the time, to avoid the exact
boundary violation this file has caught and corrected several times
already (F1 regressing B3, F3 re-asserting coverage in the receipt).
**F1 has since closed the custody half of that question via B7:**
coverage, including `unknown`, is part of the materialized semantic
output itself, committed by `output_digest` — it does not need a
receipt row to survive the module boundary, the same way `value`
doesn't need a second copy elsewhere to survive it. So this row closes
with **no field**, not because the question was skipped, but because
B7 answered it: custody is already satisfied without one. A smaller,
separate, non-blocking question — whether the receipt should
*additionally* carry a redundant coverage projection as an audit
convenience — remains F1/F4's to decide, not this row's.

**The `signature` row is now CLOSED, but narrower than a first pass
over this claimed.** This repo's own `README.md`, under "What does not
belong here," names *"Vault access · counter-signature policy · git
and release workflow..."* as explicitly out of scope for this engine.
**Decision: this engine does not perform signing, and does not decide
signer authority.** If a receipt is ever signed, Vault access and the
decision of who may sign both happen entirely outside this engine,
performed by an external authority.

**Correction (review-caught on PR #14): that does not also mean "the
receipt schema may never define a signature-related field."** A first
pass claimed exactly that, citing `cic-primitives`' D-015 as support —
but re-read directly, D-015 argues the opposite. The release bundle's
own schema **does** define a `release.sign` field (and
`tools/compiler.py`'s `run_pledge()` writes an analogous `pledge.sign`
field into `commitment.yaml`'s own schema the same way) — it is simply
*populated* by a
separate `_vault_sign()` call, made by an external authority, never by
the compiler's own build logic. **Schema ownership and producer
ownership are different facts**, and D-015 is a working example of a
schema reserving a slot for a signature it does not itself compute —
the exact opposite of "a schema that can't carry one."

Whether this receipt's own schema ever reserves such a slot (parallel
to `release.sign`), or whether a signed form is always a wholly
separate sibling artifact (e.g. a `SignedReceiptEnvelope{receipt_digest,
signer_identity, signature}` wrapping this receipt, never touching its
own schema) is **not decided here** — that is a receipt *schema
layout* question, squarely C4's territory, the same way D1's
version-identity facts were kept separate from their wire layout
earlier in this section. Deciding it now would repeat that exact
boundary violation one row over.

**A second, smaller overclaim, also corrected:** "a signature requires
Vault access AND counter-signature policy" overstates it. Producing a
signature requires an external signer/key authority (Vault access);
counter-signing is a separate, optional second layer applied *on top
of* an existing signature (`cic_countersign`, per D-015: "applied
after signing, by a different authority"), not a precondition for
producing one at all. The README names two different things this
engine stays out of, not one combined requirement.

So C3 is now decided for `BOUNDARY.md`'s own sketch (the two digests,
the two provenance-derived lists), D1's four version-identity facts
(content, not layout), C5's evidence shapes, `signature` as to WHETHER
this engine signs (CLOSED — it does not, and does not decide signer
authority), `unresolved/unknown markers` (CLOSED by B7 — no receipt
field needed; coverage is already committed via `output_digest`), and
`conformance_plan_digest`/`observation_digest` (CLOSED by F5 — neither
is a receipt field at all; both belong to the conformance/drift verdict
artifact, F3, with their own scope and byte-level semantics decided
there). What remains open is *whether the receipt schema itself ever
carries a signature-related field* (C4's call, not decided here) and
*whether the receipt should additionally carry a redundant coverage
projection* as an audit convenience (F1/F4's call, not a custody
question). C4 still owns how every one of the closed facts is actually
nested and named on the wire.

### C4. Schema home — CLOSED as to WHICH repo, and the core field layout is now written (`docs/RECEIPT-SCHEMA.md`); extension/meta fields stay open

The receipt needs a **formal, versioned schema both languages implement
against** — the same cross-language concern driving this whole effort.
**Decided:** it does **not** live in `cic-primitives`' `schemas/atomic/`
or `schemas/aggregate/` — C1's own reasoning (BOUNDARY.md's regress
argument) is specifically about the receipt NOT being a primitive/node,
so giving it a primitive's schema home would reintroduce the exact
category error that reasoning exists to avoid.

**This section previously deferred the whole question — "whether its
schema lives in this repo or needs its own location" — until C3's
field list was complete, reasoning "schema-ing a partially-known field
set would bake in gaps."** That reasoning is sound for one of the two
things "schema home" was asking, but not the other, and conflating
them is why the whole question sat open longer than it needed to:
*writing the schema's actual field list* genuinely cannot happen before
C3 completes (at the time this was written, C3 still had two
F-territory rows open — `conformance_plan_digest`/`observation_digest`
— blocked on F). *Deciding which repository hosts that eventual
schema* is a coarser, organizational fact that doesn't depend on
knowing every field — the same kind of distinction D1 drew between
WHICH version-identity facts must exist and HOW they're nested on the
wire (C4's own job, elsewhere in this section), or B7 drew between
semantic membership and physical layout. Splitting the two lets the
repo question close now, without waiting on F.

**Update (F5): those two rows have since closed, and not the way this
paragraph's "cannot happen before C3 completes" framing implied.**
They didn't get filled in with content — F5 confirmed neither is a
receipt field at all; both belong to the conformance/drift verdict
artifact (F3), not this schema. So C3's field list for THIS schema was
already complete once the version-identity row, C5, signature-as-to-
whether-this-engine-signs, and unresolved/unknown-markers closed — the
two F-territory rows were never going to add a field here, only remove
themselves from the list once their actual home was confirmed. *Writing
the schema's actual field list* is therefore no longer blocked on
anything in C3; it remains undone only because nobody has written it
yet, same as before this update.

**Decided: the schema lives in this repo (`cic-primitive-engine`), not
a new or separate location.** Three already-closed or already-stated
facts converge on this, not a single guess:
- **A0's meta-decision** (closed), the strongest of the three on its
  own: *"the materialization library is the single semantic
  authority... not a second, competing contract to keep alive
  indefinitely."* The receipt is an artifact produced by, and
  describing, this engine's own materialization semantics — so the
  schema describing it belongs to the same single authority as
  everything else that semantics covers. Hosting it anywhere else
  would mean the authority that defines the receipt's content and the
  repository that owns its schema disagree about who's in charge of
  it.
- **Correction (review-caught on PR #18): a second argument originally
  claimed the receipt "needs none of" `cic-primitives`' signed,
  Vault-backed release pipeline — that overreaches past what C3's
  `signature` row actually closed.** That row closed only that *this
  engine* does not sign and does not decide signer authority; it left
  genuinely open whether the receipt's own schema reserves a slot for
  an externally-populated signature (parallel to `release.sign`) —
  schema ownership and producer ownership are different facts, per
  D-015, which is exactly why PR #14 refused to claim the schema
  "carries no signature field." Claiming the receipt needs *none* of
  that machinery risks being contradicted the moment C4 decides that
  slot question either way. **The actually-sound version of this
  argument doesn't depend on that undecided question at all:** semantic
  ownership (who defines what the receipt's fields mean) and signature
  *production* (who, if anyone, ever signs it) are orthogonal.
  Whichever way C4 eventually decides the signature-slot question,
  external signing does not transfer ownership of the receipt's
  *semantic* schema to whatever authority performs that signing — the
  same way `cic-primitives`' own schema isn't owned by Vault just
  because Vault signs its releases. This repo defines the semantics
  either way.
- **`docs/PRIMITIVE-IR.md` is precedent for this repo owning the
  contract for an artifact it produces — not for a formal schema
  already existing here.** Correction (review-caught): `PRIMITIVE-IR.md`
  itself opens with *"Not specified yet. This file states what the IR
  must satisfy, so that the constraints are fixed before the
  representation is"* — it is a required-properties/architectural
  contract, not a working formal schema; no formal schema exists yet
  for either artifact. The precedent this actually supports is narrower
  but still real: the IR's *contract* is already, unquestionably, owned
  by this repo, and the receipt is `PRIMITIVE-IR.md`'s own sibling
  artifact (C1) — produced by the identical pipeline, for the identical
  consumers. Treating the receipt's contract as belonging elsewhere
  would need a specific, stated reason; none of the three facts above
  supplies one.

**Update: the schema's actual field enumeration has since been
written.** `docs/RECEIPT-SCHEMA.md` is C4's field-by-field text — every
field cites the decision requiring it (C1/C2, C5, D1, B7), with the
version-identity facts laid out flat (one key per D1 identifier, not
nested under a shared `version` key, for the reason D1 itself
established: the four facts vary independently, and nesting them would
visually imply they don't). Writing it out also surfaced, and resolved,
a latent overlap this table's own two rows hid: `schema.digest` (the
row above, from `BOUNDARY.md`'s pre-D1 sketch) and D1.3's richer
three-part domain-schema-identity group were never reconciled as the
same slot — `RECEIPT-SCHEMA.md` absorbs the former into the latter
(`schema_digest`, one field, not two). It deliberately omits the
signature field and the coverage-projection field (both still open,
C4's and F1/F4's own calls respectively, not decided by writing the
layout) — and surfaced one small, new, genuinely open question in
doing so: whether each receipt INSTANCE needs its own schema-version
field (distinct from this document itself having a version), which
nothing already decided settles. Added to this section's open items
below, not invented an answer to.

**Correction (review-caught on PR #24): listing field names and types
is not the same as a byte-level wire contract — the same gap F5 had to
fix one section up, recurring here.** The first version of
`RECEIPT-SCHEMA.md` never said whether `applied_defaults`/
`derived_values` may be omitted when empty, nor in what order their
entries (or `derived_values[].inputs`) appear — and section A's A6
deliberately doesn't sort `Seq`s, so without a stated rule two
conforming implementations could emit different bytes for the same
semantic receipt. Fixed: every field is now REQUIRED (extending
`PRIMITIVE-IR.md`'s already-decided Complete property to this sibling
artifact — an omitted key is exactly the kind of ambiguous gap that
property forbids), the two evidence arrays are required-but-possibly-
`[]`, both arrays sort by `.path` byte-wise (A2's comparator, reused),
and `derived_values[].inputs` is decided — newly, not inherited from
C5, which only ever said what the list *means*, not whether its order
or duplicates are significant — to be a deduplicated, sorted path set.

**Also corrected: calling the whole of "C4's actual schema text"
CLOSED overstated it.** The *core field layout* — the fields above,
now with requiredness/cardinality/ordering fixed — is closed. Three
schema-shape questions (signature slot, redundant coverage projection,
receipt-instance schema-version) remain genuinely open, and a reader
could reasonably take "schema text: CLOSED" to mean those were settled
too, or be confused when a future field gets added to a document
already called closed. Restated as: **C4's core field layout is
CLOSED; C4's extension/meta fields are PARTIALLY OPEN** — matching how
every other PARTIALLY DECIDED section in this document already
separates what's settled from what isn't, rather than introducing a
third status category.

**Still open, and genuinely so — not decided by the above:** the exact
file path/directory within this repo (a small implementation detail,
not blocking — `docs/RECEIPT-SCHEMA.md` is where it landed, not a claim
that no other path was possible), the signature-slot question, the
coverage-projection question, and the receipt-instance schema-version
question just found.

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
  it actually read to produce this result (BOUNDARY.md's own example:
  "effective-state-v1" from admin_state/oper_state). It hands the
  receipt-builder:

    DerivedEvidence { rule: "effective-state-v1", inputs: [<the exact
                       field paths this invocation actually read>],
                       value_digest: <A7 digest of the derived value> }

  Rule identifiers are stable, versioned strings (name + "-v" + number,
  per BOUNDARY.md's own example) -- a rule's logic may evolve under a
  NEW name/version without silently changing what an old receipt's
  entry means, the same versioning discipline D1 applies to grammar
  and schema identity. `inputs` records the exact paths THIS
  INVOCATION actually consumed, not a static property declared once by
  the rule's definition -- the only corpus example (effective_state)
  happens to read a fixed pair of paths every time, so for it the two
  coincide, but that is a property of this one rule, not a constraint
  this decision places on every future rule. A rule whose consumed set
  varies by invocation (e.g. "aggregate all members matching selector
  X") still produces evidence the same way: `inputs` is whatever paths
  that specific run actually read, because the evidence's job is to
  prove what fed THIS result, not to restate what the rule could in
  principle read.

  `value_digest` makes `DerivedEvidence` symmetric with
  `DefaultEvidence` rather than silently lacking it: the receipt's
  top-level `output_digest` (BOUNDARY.md's receipt skeleton) already
  binds the complete canonical output, but that is a whole-document
  commitment -- it cannot be used to verify one specific derived
  entry's value without re-canonicalizing and extracting that path
  from the full document. A per-entry `value_digest`, exactly like
  `applied_defaults` already has, lets a single `derived_values` entry
  be audited on its own. `rule` + `inputs` establish *causality* (what
  produced this entry); `value_digest` establishes *what it produced*
  -- two different, both useful, facts, not a redundant restatement of
  `output_digest`.

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
rule, inputs, value_digest}`, produced inline by the same
materialization step that applies a default or runs a derivation,
kept as a C-owned companion fact rather than folded into B3's already-
closed `provenance` enum; `inputs` is the actual paths this invocation
read, not a static rule-definition property, and `value_digest` keeps
both evidence shapes symmetric rather than leaving derived entries
unverifiable on their own), C3's `signature` row **as to whether
this engine signs** (CLOSED — it does not, and does not decide signer
authority; this repo's own README excludes Vault access/counter-
signature policy from this engine's scope), C3's
`unresolved/unknown markers` row (CLOSED, by B7 — added later, while
closing F1: coverage, including `unknown`, is already committed via
`output_digest` as part of the materialized semantic claim, so this
row needs no field of its own in the receipt), and **C4's WHICH-REPO
question** (CLOSED — the schema lives in this repo,
`cic-primitive-engine`, not a new or separate location: A0's single-
semantic-authority meta-decision as the strongest reason on its own;
semantic ownership and signature *production* being orthogonal, so
however C4 eventually settles the still-open signature-slot question,
external signing never transfers ownership of the receipt's semantic
schema to whoever signs it; and `PRIMITIVE-IR.md` already establishing
that this repo owns the *contract* for an artifact it produces, the
receipt's own sibling per C1 — narrower precedent than "a formal
schema already lives here," since no formal schema exists yet for
either artifact, but real).

**Open:**
- **Whether the receipt schema itself ever carries a signature-related
  field**, populated by an external authority the same way
  `cic-primitives`' own `release.sign`/`pledge.sign` are — a review on
  PR #14 caught that an earlier pass overclaimed this as settled
  ("the schema carries no signature field"), when D-015 actually shows
  schema ownership and producer ownership are different facts. This is
  a receipt *schema layout* question, C4's territory, not resolved by
  "this engine doesn't sign."
- **Whether the receipt should additionally carry a redundant coverage
  projection**, purely as an audit convenience — not a custody
  question any more (B7 settled that), but F1/F4's smaller, remaining
  one.
- **Whether each receipt instance needs its own schema-version field**
  — found while writing `docs/RECEIPT-SCHEMA.md`: "a formal, versioned
  schema" (this section's own words, above) is ambiguous between the
  *document* having a version and every *instance* carrying one, and
  nothing already decided settles which. New, small, not invented away
  by writing the text.

**Closed (added by F5, after this section's original closure):**
- C3's `conformance_plan_digest`/`observation_digest` rows — neither is
  a receipt field. Both belong to the conformance/drift verdict
  artifact (F3), with their own scope and byte-level semantics decided
  there, not here.

**Closed (added while writing `docs/RECEIPT-SCHEMA.md`, then tightened
on review — PR #24):**
- **C4's core field layout.** Every decided field laid out, cited to
  its source decision, with the version-identity facts flat (one key
  per D1 identifier) rather than nested — nesting would visually imply
  the four facts vary together, which D1 established they don't. Also
  resolved, in the writing: `schema.digest` (C3's table, from
  `BOUNDARY.md`'s pre-D1 sketch) and D1.3's richer three-part domain-
  schema-identity group were never reconciled as the same slot; they
  are now one field (`schema_digest`), not two. **Review caught that a
  field list alone isn't a byte-level contract** (the same gap F5
  fixed one section up): every field is now REQUIRED, the two evidence
  arrays are required-but-possibly-`[]`, both sort by `.path` byte-
  wise, and `derived_values[].inputs` is decided, as a new fact not
  inherited from C5, to be a deduplicated sorted set, not an
  execution-order trace. "C4's schema text: CLOSED" is corrected to
  "C4's **core field layout**: CLOSED" — the *extension/meta* fields
  (signature slot, coverage projection, receipt-instance schema-
  version) stay open, listed above, not swept in by the broader claim.

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
         the receipt-transport wire shape); whether native and WASM
         modules SHOULD get the SAME trust/policy treatment inside the
         mandatory boundary remains a normative open question, though
         it is now confirmed, by reading the code, that they do not
         even share the same MECHANISM today (native has no identity
         parameter at all; WASM has a meaningless one); how the ACL
         actor identity gets threaded through was a named gap, now
         checked directly against the code -- confirmed no existing
         Relay mechanism is available to wire in (iac.Actor has zero
         production call sites; SetPayload, the external API's own
         entry point, carries no identity field), so this needs
         building, not finding -- designing it remains open; and the
         guest-side defense-in-depth digest check's shape.
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
self-filter — `authContextJson` needs to carry a real identity.

**The hedge this section first left — "whether Relay's existing
auth/identity system already has one available at this call site, or
needs one built, wasn't checked" — is now checked, directly against
the code, not assumed:**
- `core/nexus/iac/acl.go`'s `Actor{User, Groups}` (A0.1, landed and
  tested) is the ONLY actor-identity type anywhere in the repo —
  checked by grepping every `.go` file for its construction. It has
  **zero production call sites.** The only places an `Actor` value is
  ever built are its own unit tests (`acl_test.go`). A tested,
  well-defined type exists; nothing in this codebase has ever
  instantiated one for a real request.
- The gap goes further back than `service.go` itself: `SetPayload`
  (`core/cabinet/set_schema.go`), the external API's own request
  struct — the very top of this whole call chain — carries
  `WorkflowID`, `Modules`, `Payload`, `Options`, `SourceDigest` and
  nothing resembling a caller identity. There is no point between the
  external API boundary and `service.go`'s dispatch loop where a real
  actor identity could be picked up from something already present —
  it would have to be added at the API boundary itself, not merely
  threaded through from somewhere nearby.
- **Decided: this needs to be built, not found.** No existing
  Relay auth/identity mechanism is sitting unused nearby, waiting to be
  wired in — the search for one came back empty. This still does not
  design the mechanism (that's real follow-up work, not a documentation
  decision), but it replaces "wasn't checked" with a verified negative
  result, so nobody re-does this search expecting to find something
  this document already looked for and didn't find.

**A second, related finding, also checked directly against the code:**
the native dispatch branch (`service.go`'s `moduleDesc.NativeImpl`
case) does not receive `authContextJson`, or anything like it, at all
— its call signature (verified via the `reflect` call-shape check at
that branch) is exactly `func(context.Context, interface{}) (T,
error)`, with no slot for an identity argument. So today, native and
WASM modules do not merely get *different trust/policy treatment*
inside the mandatory boundary (E1's still-open question) — they do not
even have the **same mechanism shape**: WASM carries a (currently
meaningless) identity-shaped string, native carries nothing at all.
This is a factual asymmetry, found, not a normative answer to whether
the two paths *should* end up symmetric once a real identity exists —
that design question stays open, but it is no longer an
undocumented one.

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
  path bypasses the chokepoint itself) — not decided either way, though
  it is now confirmed, by reading `NativeImpl`'s own call-shape check,
  that the two branches don't even share a mechanism today: `NativeImpl`
  has no identity-carrying parameter at all, where `WasmCode` has a
  meaningless one.
- How `authContextJson` becomes a real actor identity — a genuine,
  found-not-invented gap, **now checked, not merely flagged:** grepping
  the whole repo for `iac.Actor{` construction found zero production
  call sites (only its own unit tests), and `SetPayload` — the external
  API's own request struct, the top of this entire call chain — carries
  no identity field either. There is no existing mechanism sitting
  nearby to wire in; this needs building from the API boundary down,
  not discovering. Left for whoever owns that build, same as before —
  this section narrows *what* is missing, not *who* builds it.
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
         direction-agnostic), F1-custody (coverage does NOT need
         projecting into the receipt to survive the wire -- B7 already
         commits it via output_digest, as part of the materialized
         semantic claim itself, closing the custody half of this row),
         F2 (core/nexus/iac's compare.go/observation.go/conformance.go
         are migration source per A0's existing meta-decision, not a
         permanently separate contract -- made explicit for the output
         axis, not newly decided), F5 (conformance_plan_digest's and
         observation_digest's scope AND byte-level semantics: both are
         conformance/drift-verdict-artifact fields, never receipt
         fields; conformance_plan_digest digests the executed
         comparison plan, no landed precedent; observation_digest
         digests the full validated observation claim the comparator
         consumed -- coverage envelope AND observed values -- a
         deliberate widening of Relay's landed observationDigest(),
         which digests only the coverage envelope).
Open:    whether the receipt should ADDITIONALLY carry a redundant
         coverage projection as an audit convenience (not a custody
         requirement any more, per B7) -- a smaller, non-blocking
         question than the one this row used to pose. Also open: the
         comparator/verdict EXECUTION LOGIC (doesn't exist in the new
         lib yet, in either language -- F5 decided what the two digests
         commit to, not how to compute a verdict), and how the
         conformance/drift verdict relates to the receipt (C) and to
         ProofTrace (E's finding) -- three adjacent, distinct proof
         artifacts, not one.
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
  territory, open at the time this was written (closed since, by F5
  below). So: `MaterializedField.coverage` (B3) is
  in-memory state, already decided. Whether — and how — that coverage
  information gets *projected into the receipt* for an observed field was
  left as a separate, open question here — **now closed, by B7, added
  while closing this very row:** coverage does not need the receipt to
  survive the module boundary at all. B7 establishes that
  capability/coverage/provenance are part of the materialized semantic
  output itself, committed by `output_digest` — the same mechanism that
  already protects `value`. The receipt's own job (C1's reasoning: trace
  *where a value came from*, sitting beside the data) was never the only
  thing standing between coverage and "disappearing at the wire"; B7
  shows it was never at risk of disappearing in the first place. What
  remains genuinely open, and is smaller than the original question, is
  whether the receipt should *additionally* carry a redundant coverage
  projection purely as an audit convenience — not a custody requirement,
  since B7 already supplies that — addressed in F4 below.

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
                               evidence (C3/C5). Coverage/observation
                               evidence does NOT need to become part of
                               this artifact for custody purposes --
                               B7 already commits coverage via the
                               canonical result's own output_digest,
                               the same mechanism protecting `value`.
                               Whether this artifact ADDITIONALLY
                               carries a redundant coverage projection,
                               purely as an audit convenience, remains
                               OPEN (see F1/F4) -- a smaller question
                               than this diagram originally posed.
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
mechanism claim (C2's receipt production is direction-agnostic), F1's
custody claim (coverage needs no receipt projection to survive the
module boundary at all — B7 already commits it via `output_digest`,
as part of the materialized semantic claim itself), F2 (the comparator
is migration source under A0's existing meta-decision, not a permanent
fork — explicit, not newly decided), and the three-artifact distinction
(F3).

**Not closed, deliberately:**
- **Whether the receipt should ADDITIONALLY carry a redundant coverage
  projection, purely as an audit convenience.** This is what remains of
  the row that used to read "whether/how `MaterializedField.coverage`
  gets projected into the receipt" — narrowed by B7, which answered the
  custody half (coverage doesn't need the receipt to survive the wire;
  it's already part of what `output_digest` commits to). C3's actual
  field set still carries no coverage payload today, and that is no
  longer a gap this document is waiting to fill — it's a legitimate,
  closed answer (no custody-driven coverage field), leaving only a
  smaller "nice to have for audits" question, not decided here.
- The comparator/verdict logic has no implementation in the new lib at
  all yet, in either language — this section (and F5 below) establishes
  what it produces and what the two digests commit to, not the code.
- How artifact 2 (receipt) and artifact 3 (conformance verdict) relate
  procedurally — e.g. does computing a verdict require a receipt to
  already exist, or are they independent outputs of the same
  materialization call — is not decided here.
- **B7's own wire-layout question** (exactly how capability/coverage/
  provenance are nested/named next to `value` in the canonical tree) is
  not decided here either — that's C4/E2b's call, same as every other
  field's physical placement.

### F5. `conformance_plan_digest` and `observation_digest` — scope and byte-level semantics

C3 reserved these two as "section F's territory" without saying what
they bind, beyond confirming (F1/F3) that they are not receipt fields.
This closes that: both are fields of the conformance/drift verdict
artifact (artifact 3, F3) — never the materialization receipt (artifact
2) — and this is what each one's bytes actually commit to.

**`conformance_plan_digest` — no landed precedent at all, grounded by
reading `conformance.go` directly rather than assuming the name implies
existing behavior.** `ConformancePlan`/`FieldPlan`/`CollectionPlan`
exist as Go types in `core/nexus/iac/conformance.go`, but `Evaluate()`
never passes a plan value to a digest function — only `intent` (via
`SpecDigest`) and `obs` (via the file's own `observationDigest`) are
digested. `iac-object-model.md`'s `proof` object-level index names the
field but is explicit that this whole index is the **deferred** build.
**Decision:** `conformance_plan_digest` is the canonical-bytes digest
(section A's format, reused — no new serialization rule invented) of
the compiled comparison plan actually executed for this verdict. This
lets two verdicts be compared for *plan* equality independently of
whether their *results* happen to agree, the same kind of distinction
D2's comparability gate already draws for materialization (G3).

**Correction (review-caught on PR #23): naming the semantic content
("scalar field paths and comparators, collection topologies and
per-element plans") is scope, not a byte-level contract.** Two
implementations can both honestly satisfy that sentence and still
produce different trees — `{"fields":[{"path":"$.x","comparator":
"equal"}]}` and `{"scalar_fields":{"$.x":{"compare":"equal"}}}` are
both faithful to the semantics above, and section A canonicalizes each
correctly, to **different** bytes. A digest is only a digest of
something specific; "the semantic content" is not specific enough to
digest. Fixed below with a named, exact preimage shape — a
**PlanDigestProjection** — expressed directly as a section-A `Value`
tree (`Map`/`Seq`/`Str`, from `reader.rs`), not Go or Rust struct
layout:

```text
PlanDigestProjection =
  Map {
    "scalars":     Seq[ Map{ "path": Str, "compare": Str } ],
    "collections": Seq[ Map{
      "path":     Str,
      "topology": Str,        -- "atomic" | "set" | "map"
                               -- Collection.Topology's own three values
                               -- (collection.go), the semantic content,
                               -- not the Go CollectionTopology type
      "keys":     Seq[ Str ], -- TopologyMap's key fields; ALWAYS
                               -- present, an empty Seq for "atomic"/
                               -- "set" -- never omitted, so a present-
                               -- vs-absent-key choice can't itself
                               -- change the bytes
      "elements": Seq[ Map{ "path": Str, "compare": Str } ]
    } ]
  }
```

`compare` is the literal `CompareType` string (`"exact"`/`"numeric"`,
`compare.go`) — the comparator's semantic identity, not a Go/Rust enum
tag.

**Ordering, stated explicitly because section A does not supply it for
arrays.** A2 (this document, section A) sorts `Map` keys by UTF-8 byte
order; A6 preserves `Seq` order exactly as given — it has no opinion on
array order at all, by design, because array order is sometimes
meaningful data. Here it is not: `scalars`, `collections`, each
collection's `elements`, and each collection's `keys` are all
*unordered sets* at the semantic level `ConformancePlan` actually
describes. Byte-identical digests across two implementations therefore
require a **projection-level** ordering rule, decided here, not
inherited from section A:
- `scalars`, `collections`, and every `elements` list: sorted by
  `path`, byte-wise (the same comparator A2 already uses for map
  keys — reused, not reinvented).
- `keys`: sorted byte-wise — this mirrors `Collection.ElementKey`'s own
  `sort.Strings(keys)` (`collection.go`, already landed), not a new
  rule invented for this digest.

`conformance_plan_digest = digest(to_canonical_json(PlanDigestProjection))`,
using section A's own `digest`/`to_canonical_json` (`canonical.rs`) — no
third serialization step.

**`observation_digest` — landed precedent exists, but it covers less
than the name suggests, and this decision deliberately widens past it.**
Read directly: `conformance.go`'s `observationDigest(obs Observation)`
digests only `Observation{Observed []string, AuthoritativeAbsent
[]string}` — the coverage *envelope*, i.e. which field paths were
looked at and which were authoritatively confirmed absent. It never
touches `observed map[string]interface{}`, the actual values the
comparator compared against intent to produce each field's verdict.
The file's own comment says why it was built that way: *"so coverage
itself is part of the proof"* — a narrower goal than binding the
verdict to everything it was computed from.

**Decision:** for this engine, `observation_digest` binds the canonical
bytes of the full validated observation claim the comparator actually
consumed — the coverage envelope *and* the observed value at every
covered path — not only the envelope. **This is an explicit, named
divergence from Relay's landed behavior, not a recovered fact,** and
the reason is the same one B7 already established: a materialized
semantic claim's constituent facts must be committed as real,
digested data, not merely present at evaluation time and then
discarded. Under the landed, envelope-only digest, two evaluator runs
with identical coverage (the same paths observed/absent) but silently
different observed values at those paths would produce the *same*
`observation_digest` while potentially producing *different*
per-field verdicts — the digest would then certify "these paths were
looked at" while saying nothing about what was found there, which is
exactly the gap B7 closed for `value`/`coverage`/`provenance` in the
materialized tree itself. Binding the digest to the full consumed
claim closes the same gap for the verdict artifact.

**Correction (review-caught on PR #23, two related gaps in the same
paragraph):**

1. **"The coverage envelope and the observed value" was scope, not a
   preimage shape** — the same underspecification as
   `conformance_plan_digest` above, fixed the same way: a named,
   exact **ObservationDigestProjection**, a section-A `Value` tree.
2. **The projection must distinguish B3's actual four coverage values,
   not Relay's two-list envelope.** The paragraph above, read literally,
   still speaks Relay's `Observed`/`AuthoritativeAbsent` list language.
   But B3 (closed, this document) already decided `coverage` is
   `observed | absent | unobserved | unknown` — four values, not the
   binary "looked at or not" a two-list envelope encodes. `unknown` is
   B2's own addition with no Relay equivalent at all (B2: *"the device
   itself reported an indeterminate value... distinct from `absent`...
   and from `unobserved`"*) — exactly the B7 logic this section already
   invokes: if `unknown` isn't distinguishable in the digest from
   `unobserved`, the digest doesn't actually commit to it, which is the
   same gap B7 exists to close, reopened one level down.

```text
ObservationDigestProjection =
  Map {
    "fields": Seq[ Map{
      "path":     Str,
      "coverage": Str,   -- one of B3's four values, verbatim:
                         -- "observed" | "absent" | "unobserved" |
                         -- "unknown" -- never Relay's two-list form
      "value":    <canonical value, per section A>
                         -- key PRESENT iff coverage == "observed";
                         -- OMITTED (never present-with-null) for every
                         -- other coverage value
    } ]
  }
```

**Why `value` is restricted to `coverage == "observed"`, and not, say,
also attached to `unknown`'s indeterminate device report:** grounded in
`observation.go`/`compare.go`'s own logic, not invented for this
digest. `ClassifyFieldValue`: *"if `o.Coverage(path) != CoverageObserved`
{ ...the observed value is not authoritative, so the comparison is
irrelevant — coverage alone decides }"* — the comparator itself never
reads a value for any coverage state but `observed`. This projection
commits to what the comparator actually consumed (F5's own framing,
above); it is not a restatement of `MaterializedField`'s separate,
already-closed (B3) value-presence rule, which is a different
question about a different artifact (the materialized tree, not the
verdict). A future decision could add a raw-report payload for
`unknown` if the comparator itself ever starts consuming one — not
decided here, because nothing consumes one today.

**Ordering, for the same reason as the plan projection:** `fields` is
sorted by `path`, byte-wise, before canonicalization — section A's A6
preserves `Seq` order as given and does not sort it; the projection
supplies its own rule here, reusing A2's byte-wise comparator rather
than inventing a second one.

`observation_digest = digest(to_canonical_json(ObservationDigestProjection))`,
using section A's own `digest`/`to_canonical_json`, same as the plan
digest.

**What this does not do:** it does not implement the comparator (F1's
gap stands, in both languages) — these two projections are inputs a
future comparator implementation must construct and digest the same
way in both languages, not comparator code itself. It does not decide
how `conformance_plan_digest`/`observation_digest` are nested or named
on the conformance-verdict artifact's own wire format — that artifact
has no schema-layout decision yet, the same way the receipt's C4 layout
is separate from C3's field-content decisions. It only settles what
each digest's bytes are taken over, now at the same byte-level
precision section A already holds materialization to.

## G — Differential conformance (PARTIALLY DECIDED, not closed)

```text
Status: PARTIALLY DECIDED, not closed -- and cannot fully close until
F's comparator implementation lands (B2, C5, C3's unresolved/unknown-
markers row, and C3's conformance_plan_digest/observation_digest rows
have all since closed in later passes and no longer belong on this
list). This section fixes the comparison HARNESS's structure and two
scoping questions; it does not, and cannot yet, specify full test
coverage.

Closed:  G1 (the existing conformance/ corpus is extended, not
         duplicated), G2 (check_grammar.py stays out -- confirmed
         deliberate, not a gap), the harness structure (G3): gate
         first using D2's comparability check, then compare canonical
         bytes (A), then compare receipt semantics excluding engine
         identity (D1.4's forward note) -- three separate questions,
         never one byte-equality check on everything.
Open:    full coverage. F's not-yet-implemented comparator is
         explicitly out of the harness's scope until it lands -- not
         silently skipped. Anything exercising conformance_plan_digest/
         observation_digest is included in that same exclusion, now for
         a narrower reason than before: F5 decided what the two
         digests commit to, but nothing computes a verdict (or either
         digest) in either language yet, so there is still nothing to
         vector against. C5's evidence record and C3's unresolved/unknown
         markers row are no longer on this list: C5 closed with a
         specified shape (DefaultEvidence/DerivedEvidence), and the
         markers row closed via B7 (coverage is part of the canonical
         byte comparison step A already performs, with no separate
         receipt field needed) -- neither is a harness-scope blocker
         any more.
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
   projection, not a wire-level field removal** -- C4's exact field
   layout is still open (only WHICH repo hosts the schema has closed),
   so there is no fixed field path to name "remove" yet; specifying one
   here would decide C4's layout by accident, from inside G, before C4
   itself closes.

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

**Update: by the time this is read, every C3 field-semantics row has
closed** (`unresolved/unknown markers` via B7, `conformance_plan_digest`/
`observation_digest` via F5, C5's evidence record with a specified
shape) — none of C3's rows is "semantics not yet decided" any more.
Neither step 2 nor step 3 can be reached for a vector exercising the
comparator/verdict artifact specifically, but for a different reason
now: F's comparator has no implementation in either language, so there
is nothing yet to run such a vector against. Such a vector is OUT OF
SCOPE for this harness today, named as a gap, not silently treated as
passing or skipped without record.
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
- Full test coverage — blocked on F's comparator/verdict implementation
  (doesn't exist in the new lib in either language). C5's evidence-
  record shape, C3's `unresolved/unknown markers` row (closed via B7:
  coverage is part of what section A's canonical-byte comparison
  already catches, no separate receipt field needed), and C3's
  `conformance_plan_digest`/`observation_digest` rows (closed via F5:
  scope and byte-level semantics decided, as conformance-verdict-
  artifact fields) have all since closed and no longer block this. The
  harness structure is ready to run the moment the comparator exists;
  it cannot run completely before then.
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

## Addendum: `PRIMITIVE-IR.md`'s open question #2

`PRIMITIVE-IR.md`'s own five open questions seeded part of this
document (#1 → C1, #3 → A, #4 → D); #2 was never picked up by any
lettered section, and stayed unanswered even though the answer already
existed. Recorded here rather than forced into a section it doesn't
naturally belong to — folding it into B, in particular, would wrongly
imply it fits inside B1's already-closed three-axis count, which it
does not necessarily do.

**Answered, but by reconciling a real internal tension in
`BOUNDARY.md` — not a clean, uninterpreted recovery.** `PRIMITIVE-IR.md`'s
open question #2 asks *"Does the IR carry unresolved references
explicitly, or is resolution total?"* `BOUNDARY.md`'s own Constructors
section states, as one of `Validated<Materialized<PrimitiveDocument>>`'s
guarantees: *"every reference is resolved or explicitly unresolved."*
Not "every reference IS resolved" — the type names and permits the
other outcome. **Correction (review-caught on PR #17): citing that
sentence alone and calling the question "recovered, not newly decided"
overstated it** — `BOUNDARY.md` contains two *other* sentences that, on
a first read, pull the other way:
- the forbidden-states list: *"the states below must not be
  representable on the module side... an object with references still
  unresolved"*;
- materialized validation's own requirement: *"references resolvable."*

If "unresolved" in the forbidden list and "explicitly unresolved" in
the constructor guarantee are the *same* state, `BOUNDARY.md`
contradicts itself outright — forbidding on one page exactly what it
guarantees as acceptable on the next. **Resolving that, not assuming
it away, is this addendum's actual decision:** the forbidden list's
*"references still unresolved"* describes resolution **as an
in-progress, incomplete process state** — the same register as its
list-mates *"a partially interpreted document"* and *"defaults still
unapplied"*, every one of them a thing that hasn't finished happening
yet. The constructor's *"explicitly unresolved"* is a different thing
entirely: resolution **ran to completion** and reached a determinate,
terminal answer — *this reference does not bind to anything* — the
same way a terminated computation that returns `None` is not "still
running." So:

```text
still unresolved       -- resolution incomplete/pending -- FORBIDDEN
                           at the module boundary
explicitly unresolved  -- resolution ran, terminal answer: no binding
                           -- a legitimate materialized outcome
resolved                -- resolution ran, terminal answer: bound
                           -- a legitimate materialized outcome
```

**`references resolvable` is named explicitly as ambiguous wording,
not silently read past.** Taken at face value ("every reference CAN be
resolved," i.e. must succeed), it flatly contradicts the constructor
guarantee one section up, which names a second legitimate outcome by
name. This decision reads `resolvable` as shorthand for *"resolution
has reached a terminal result"* (bound or explicitly not), not as *"the
target must exist"* — favoring the constructor guarantee's more
specific, explicit wording over materialized validation's looser
restatement of the same requirement, on the reasoning that the
constructor is the actual type-level contract `ModuleInput` enforces,
and the validation bullet is describing that same contract in prose,
not adding a stricter one. This is **this document's own reading**,
not a fact `BOUNDARY.md` states unambiguously; if `BOUNDARY.md` is
ever revised, tightening `resolvable`'s wording there would remove the
need for this reconciliation entirely.

**With that distinction drawn: resolution is not required to be total
for a document to be valid.** An explicitly-unresolved reference,
reached as resolution's own terminal, determinate output — not a
pending or incomplete state — is a legitimate, representable
materialized outcome, not a failure mode that blocks materialization.

**Named so it is not conflated with a different, separate gap:** this
is not `cic-primitives`' own D-014, which left reference
*target-existence* checking unbuilt (*"there is no type registry that
could say whether `cic:network:NetworkInterface` is a Kind that
exists"* — D-014, explicitly scoped as "a separate item," not this
one). D-014's gap is static and schema-level: is the declared target
even a real Kind? This question is instance-level and
materialization-time: does *this specific* authored reference, within
*this* composition, resolve to another object in that composition's
own graph? Different axis, the same category of conflation this
document has already named and avoided elsewhere
(`canonicalNumber`/`normalizeNumbers`, `missing` in
`cic-schema-registry`'s `coverage.py` vs. `BOUNDARY.md`'s coverage).

**What this does NOT decide, and should not be read as deciding:**
exactly how "explicitly unresolved" is represented — whether it is a
new value on an existing axis, a dedicated field, or something B1's
three-axis model would need revisiting to accommodate. Settling that
the *outcome* must be representable at all, and distinguishing it from
an in-progress "still unresolved" state, is not the same as having
designed its representation; that remains genuinely open, for whoever
picks it up next. Nor does this touch `BOUNDARY.md` itself — the
reconciliation above is this document's own reading of an existing
tension, recorded here, not an edit to the source it reads.
