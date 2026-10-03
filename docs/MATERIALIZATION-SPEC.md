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

## B — Semantic state model (closes section B)

**Decision:** the "five distinct statements" `docs/BOUNDARY.md` names
(`missing`, `unknown`, `not_observed`, `not_implemented`, schema-applied
default) are not five values of one enum — they are points on **three
separate, orthogonal axes**. Collapsing them onto one axis is exactly the
mistake `BOUNDARY.md` warns against; keeping them on three means a field
can independently be, say, `not_implemented` (capability) **and**
`unobserved` (coverage) **and** `authored` (provenance) at once, each
fact recorded separately rather than forced into one slot.

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
missing                coverage     absent, OR: the key is not present in
                                     the materialized object at all --
                                     see the open question below
unknown                ???          not grounded anywhere in either repo
                                     -- see below, this is a new proposal,
                                     not a recovered fact
```

**`missing` needs a named caveat.** `BOUNDARY.md` uses "missing" once, in
passing (*"a missing observation must not be masked by an invented
one"*), without formally distinguishing it from `unobserved`/`absent`.
Grepped both `cic-primitives` and this repo: **no decision record defines
`missing` as a state distinct from the coverage axis's `absent`.** Treated
here as the same concept as coverage's `absent` (an observe call looked
and the field is authoritatively not there) — not a fourth coverage value.
**This is a judgment call, not a recovered fact — flagging for review
rather than asserting it quietly.**

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
    value:       <canonical value, per section A>
    capability:  implemented | not_implemented | deprecated
    coverage:    observed | absent | unobserved | unknown   (state/output side only --
                                                               see note below)
    provenance:  authored | schema_default | derived          (intent/input side only)
}
```

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

### B5. `default_injection` is a response-time filter, not a stored second value

Per `access.yaml`'s own semantic mapping (`miss_behavior: default_injection
returned when access is denied`; *"fields the requestor cannot access are
filtered from the... response or returned with their default_injection
value"*): `default_injection` is **not** a second value held inside
`MaterializedField` alongside the real one. The materialized form always
holds the real value and its full B1 metadata; `default_injection` is
computed **at response-construction time**, parameterized by the
requesting actor's ACL check (`core/nexus/iac/acl.go`'s `ACL.Allows`,
already landed) — if the actor fails the read check, the response
substitutes `default_injection` for that field; the internally-held
materialized value is never touched or duplicated.

```text
MaterializedField.value  -- always the real value, always present internally
  ↓ (at response time, per-requester)
ACL.Allows(actor, PermRead)?
  yes → emit MaterializedField.value
  no  → emit access.default_injection  (from the field's long-form descriptor)
```

### B6. What this does and doesn't close

**Closed:** the three-axis model (B1), the mapping of `BOUNDARY.md`'s
named terms onto it with one explicit judgment call (`missing` ≡
coverage's `absent`) and one explicit new proposal offered for review
(`unknown` as a fourth coverage value), the materialized-type shape (B3),
the resolution of A0.4's conformance conflict as a named lossy projection
(B4), and `default_injection`'s place as a response-time filter, not
stored state (B5).

**Not closed, deliberately:**
- The `unknown` proposal in B2 is explicitly flagged as unverified
  reasoning, not a recovered fact — review before treating it as settled.
- Section C (receipt schema) still has to decide how `provenance`
  (B1/B3) relates to the receipt's own `applied_defaults`/`derived_values`
  fields — are they the same data surfaced twice, or does the receipt
  derive from the per-field provenance, or vice versa? Not decided here.
- Section E (boundary enforcement) still has to decide what prevents a
  module from reading `MaterializedField.value` directly, bypassing B5's
  response-time `default_injection` substitution, for a module that is
  itself an untrusted requester (not just the external API caller this
  section assumed). Not decided here.
