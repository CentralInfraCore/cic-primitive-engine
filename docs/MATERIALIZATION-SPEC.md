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
