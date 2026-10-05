# Open questions: Go/Rust dual materialization

```text
Purpose:
Define the unresolved decisions required before implementing
dual Go/Rust primitive materialization with byte-identical
differential conformance.

Rule:
No implementation decision should silently resolve an item in this file.
Each closed item must point to the corresponding decision/issue/PR.
```

## Why this exists

Two independent implementations are planned: a Rust library (reused by the Go
relay host through the existing `ffi/` cgo airlock — see `CIC-Relay/ffi/`) and
a Go library (for Go-authored WASM provider modules, which cannot reach the
host's cgo-linked Rust across the WASM sandbox boundary). Both must resolve
the same CIC primitive short/long form (`key: value` vs. `key: {value,
access, modify, inherit, default_injection, conformance, ...}`, see
`cic-primitives/schemas/atomic/access.yaml`) into the same materialized
result.

"The same result" is not yet defined precisely enough to test for. This
engine's own README names the exact historical failure this file exists to
prevent: *"two implementations agreed semantically on all thirteen
materialization vectors and matched on zero of them byte for byte, because
canonicalization was never written down."* Everything below has to be
answered before either library is more than a parser.

## Materialization contract (context for every section below)

```text
authored input
   ↓
authoring validation
   ↓
normalization (short form → long form; schema defaults by Role axis)
   ↓
default / derivation / reference resolution
   ↓
materialized validation
   ↓
canonical bytes
   ↓
receipt
```

This is `docs/PRIMITIVE-IR.md`'s `Read → Parse → Normalize → Resolve →
Validate → Canonicalize` pipeline, named for where a receipt attaches.
Sections A0 and A–G below are this pipeline's open decisions, in the order
they must be closed — each later section assumes the ones before it are
settled.

## Decision order: A0 → A → B → C → D → E → F → G

**A0, A and B are now closed** (2026-10-03, B as of a later pass — see
`docs/A0-INVENTORY.md` and `docs/MATERIALIZATION-SPEC.md`). B's
`missing`/`unknown` question (B2) is resolved: `missing` ≡ coverage's
`absent` (closed by cross-reading `PRIMITIVE-IR.md`'s independent
three-reason enumeration against `BOUNDARY.md`'s five terms, not by
repeating the earlier, correctly-rejected bare assertion), `unknown` is
adopted as a new, fourth coverage value, and B1's three-axis count is
confirmed final — no fourth axis needed. (An earlier version of this
file briefly claimed B was fully closed while its own B2 said OPEN, a
real self-contradiction review caught at the time; B2 genuinely closes
now, with the argument recorded, not asserted again.)

**C was built on B's closed parts (B1/B3/B4/B5) before B2 closed**, and
remains partially decided for its own, separate reasons: C1/C2 are
settled, C3's version-identity row is now also closed (D1's four
identifier groups filled it in, once D closed — see below), C5 is now
closed in full (classification — provenance alone decides list
membership — **and** the entry-evidence payload: `DefaultEvidence
{rule, value_digest}` for schema defaults, `DerivedEvidence{rule,
inputs, value_digest}` for derivations, produced inline by the same
materialization step that applies the default or runs the derivation,
never folded into B3's closed `provenance` enum — `inputs` is the
exact paths that invocation actually read, not a static property
declared by the rule's definition, and `value_digest` keeps the two
evidence shapes symmetric), and C3's `signature` row is now also
closed **as to whether this engine signs** — not by finding a
precedent for adding a receipt field, but by finding that this repo's
own boundary already excludes signing itself: the README's "what does
not belong here" names Vault access and counter-signature policy as
out of scope. **This engine does not sign, and does not decide signer
authority.** A review on PR #14 caught that a first pass over-reached
from there into "the receipt schema may never carry a
signature-related field" — `cic-primitives`' own D-015 shows the
opposite: its release bundle's schema *does* define a `release.sign`
field, populated by a separate `_vault_sign()` call made by an
external authority, never by the compiler's own build logic. Schema
ownership and producer ownership are different facts, so *whether
this receipt's schema ever reserves a slot for an externally-produced
signature* is left open, as C4's own schema-layout question, not
decided by this closure. C3's `unresolved/unknown markers` row has
since closed too, by B7 (added later, while closing F1): coverage is
part of the materialized semantic claim `output_digest` already
commits to, so this row needs no field of its own — not B2's or C3's
call, but B7's, the same way the row was always going to be decided
somewhere else, not here. **C4's own WHICH-REPO question has since
closed too** (added later, while closing C4): the schema lives in this
repo, not a new or separate location — see C4's own entry below for
the three convergent reasons. **C3's `conformance_plan_digest`/
`observation_digest` rows have since closed too, by F5:** neither is a
receipt field at all — both belong to the conformance/drift verdict
artifact (F3), with their own scope and byte-level semantics decided
there, not in this schema. What's left: the signature-field layout
question just named, plus C4's actual schema *text* (not which repo
hosts it, and no longer blocked on any unresolved field), reserved for
whoever writes it next.

**D is now closed too** — four identifiers, not one or two (grammar
digest, primitive release identity, domain schema identity — logical
name + version + content digest, three parts, not a bare version
number — and validator/engine identity), filling the version-identity
row C3 reserved for it. Go and Rust release independently; the full
semantic-input identity (grammar digest + primitive release + schema
identity/version/digest + input digest, everything except
validator/engine identity, which is supposed to differ) is the
comparability gate section G's differential conformance checks before
comparing output. **E is now PARTIALLY DECIDED** — host-side enforcement
at a specific, real chokepoint already found in `CIC-Relay/core/cabinet/
service.go` (E1, E2a, E3 closed), with the type's exact representation
and the receipt-transport wire shape left open (E2b). The dispatch
loop's `authContextJson`-isn't-a-genuine-actor-identity gap, originally
left as "wasn't checked," has since been checked directly against the
code: `iac.Actor{...}` has zero production construction sites anywhere
in the repo, and the external API's own request struct carries no
identity field either — nothing existing is available to wire in, so
this needs building, not finding. Designing it is still a standing
dependency for anyone implementing E, now with that search already
done rather than left for later.

**F is now PARTIALLY DECIDED too** — the model already covers output
symmetry by construction (no new data shape needed: B3's coverage/
provenance axes, already orthogonal not input-vs-output split, and
section A's canonical form already serve both; section C's every-call
receipt *production* is direction-agnostic), and `core/nexus/iac`'s
comparator (`compare.go`/`observation.go`/`conformance.go`) is
confirmed migration source under A0's existing meta-decision, not a
permanent fork — made explicit for this axis rather than left looking
undecided. **F1's custody question is also now closed, by B7:**
coverage does not need projecting into the receipt to survive the
module boundary — it's already part of the materialized semantic
output `output_digest` commits to. **The smaller "additionally carry a
redundant coverage projection" question B7's closure left is also now
closed — decided no**, in `docs/RECEIPT-SCHEMA.md` while closing
section C in full (no concrete audit-convenience need was ever
established). **Since settled: the comparator's object-level walk is
now implemented in Rust (F8), and how the materialization receipt
relates to the separate conformance/drift verdict (a third, distinct
proof artifact alongside ProofTrace and the receipt) is now decided —
independent (F10, `docs/VERDICT-SCHEMA.md`).** **Update: F6 and F7 now
have Go peers (F11, `go/conformance`, plus `go/canonical` as its
prerequisite; F12, `go/collection`) — each independently verified
against the same contract, not yet cross-checked against the Rust
side. Building F12 caught a real, previously-uncaught negative-zero
divergence between `collection.rs`'s Go-formatting port and real Go,
fixed in the same pass.** No Go peer exists yet for F8, F9 or F10.

**G is now PARTIALLY DECIDED too, and closes the first pass through
A–G.** The comparison harness structure is fixed: extend the existing
`conformance/` corpus rather than duplicating it; `check_grammar.py`
confirmed out of scope (different axis, recovered from A0/F); and the
actual comparison is three separate questions in order — D2's
comparability gate, then section A's canonical-byte equality, then
receipt semantic equality via a comparison *projection* that excludes
engine identity (D1.4) rather than a wire-level field removal, since
C4's exact receipt layout is still open — with its own verdict
vocabulary (`NOT_COMPARABLE`/`DIVERGENCE`),
deliberately distinct from `conformance.go`'s intent-vs-observed
verdicts (F3). What G cannot yet do is run with full coverage: any
vector touching C3's still-open fields or F's unbuilt comparator is
out of scope until those close (B2, C5 and C3's `unresolved/unknown
markers` row no longer belong on this list — all closed in later
passes, the last one via B7). Every section A–G now has at least a
decided core — B and C5 are fully closed; C's remaining rows, E and F
remain explicitly partial. The next work is closing those named items,
not opening new sections.

Section A0 closed first, ahead of even A: before picking a canonical byte
format, there was a prior-art question that would have made picking one
from scratch a mistake. Section A closed next — without a canonical byte
representation, it was not possible to even state objectively whether the
Go and Rust implementations produced "the same" output.

---

### A0. Existing implementation convergence

**Status:** DECIDED (meta-question) / **findings complete, decisions not
made** (the inventory itself — see `docs/A0-INVENTORY.md`)
**Blocks:** A — picking a canonical format from scratch would be a mistake if
a tested one already exists for most of this
**Decision ref:** `docs/A0-INVENTORY.md`

CIC-Relay already has a landed, tested implementation of a good part of this
problem, under different vocabulary, in `core/nexus/iac`
(`features/feature-011-oci-provider/iac-object-model.md`):

- `key: VALUE ≡ key: {value, default, mode: {read, write, implemented,
  visible}}` (`field.go`, `ExpandField`) — the short/long form expansion
  this file's whole premise is about. **Simpler than the Access atom's**
  `key: value ≡ key: {value, access, modify, inherit, default_injection,
  conformance}` — no embedded ACL, no `default_injection`, no `inherit`.
  `mode.implemented` partially covers `conformance` but is boolean where
  `conformance` is tri-state (see A0-INVENTORY.md's A0.4 #1). **Landed,
  tested.** The fuller `$cic`-layered model (`schema`/`behavior`/`access`/
  `collection`) is itself still a *deferred* target in that repo's own docs
  — only `node.go`'s `Meta{FieldID, Mode, Compare, Sensitive, ACL}` partially
  realizes it today.
- **spec #1, Canonical Object Encoding** — number normalization
  (`number.go` ↔ the `cic-canonical` Rust crate, byte-identical vectors,
  proven) plus map-key order / null-vs-absent (`canonicaljson.go`, Go only,
  no Rust peer yet).
- **spec #2, ACL algorithm** (default-deny + owner/group/other + named allow
  + mask + inherit) — landed, tested, Go only.
- **spec #4, Observation completeness** (`observed` / `authoritative_absent`
  / `unobserved`; verdicts `CONFORMANT`/`DRIFT`/`OBSERVED_ABSENT`/
  `UNOBSERVED`/`NOT_COMPARABLE`) — landed, Go only. This is section B's
  five-state concern, independently converged on, under different names.
- **spec #5, stable `field_id`** (survives a path rename) — landed, tested,
  Go only.
- A `proof` object-level index is *sketched* (`schema_digest`,
  `conformance_plan_digest`, `observation_digest`, `object_digest`,
  `signature`) — this file's "receipt" (section C) — but **unlike the
  bullets above, this one is not landed.** `iac-object-model.md` says so
  itself: *"the object-level `managedFields`/`observation`/`proof`
  indices are the **deferred** build."* Corrected 2026-10-03 after
  review caught that an earlier version of this bullet implied landed
  code by sitting next to bullets that are. There is no `proof.go`; what
  actually runs per call is `conformance.go`'s `Evaluate()`, which
  returns a `ConformanceResult` (an intent/state drift verdict plus two
  digests) — a different artifact from the materialization receipt this
  file means by "receipt," not a landed instance of it.
- A type-level custody boundary (`sensitive.go`): a secret field's Go type
  can only ever hold a `SecretRef`, never plaintext, enforced object-wide —
  a working, proven example of section E's enforcement problem, in Go,
  which `docs/BOUNDARY.md` had only Rust prior art for.

`iac-object-model.md`'s own audit already flagged the risk this creates:
*"The surface layer (ConfigSurface/StateSurface/BindingSurface/
ManagedEntity) ... is not in this repo — it lives in the separate
schemas.tar / cic-module-oracle-cloud artifact."* The `$cic`/`mode` model and
the `cic-primitives` Access/Role atom model have never been reconciled —
two parallel universes over the same problem.

**Meta-decision (closed):** the materialization library is the single
semantic authority — not `cic-primitives` alone, not `core/nexus/iac` alone.
`core/nexus/iac` is **migration source and tested reference material**, not
a second, competing contract to keep alive indefinitely:

```text
cic-primitive-engine / materialization lib
        ↓
one canonical semantics
        ↓
Relay · WASM guest (Go) · WASM guest (Rust) · schema tooling · proof chain
```

Agreed roadmap:

```text
1. Inventory core/nexus/iac (A0.1-A0.4 below)
2. Decide what moves over unchanged / adapted / discarded
3. Write the normative materialization spec (closes A-G)
4. Implement the library in Go + Rust
5. Differential conformance (section G)
6. Migrate Relay onto the library
7. Remove the superseded core/nexus/iac logic
```

**The inventory (step 1 above) is done — see `docs/A0-INVENTORY.md`.**
Every non-test file in `core/nexus/iac` (10 files) was read in full and
cross-checked against all 8 `cic-primitives` atomic schemas. Summary:

1. **A0.1 (migrate candidates)** — `field.go`'s short/long expansion,
   `digest.go`/`number.go`/`canonicaljson.go`'s canonical pipeline,
   `collection.go`'s topology (maps cleanly to `shape.collection_variant`/
   `item_key`), `node.go`'s `Value` type (constructor-gated, internally
   consistent — partial Go prior art for section E; `Node` itself still has
   exported fields and an opt-in `Validate()`, so the full "unvalidated
   object cannot reach a module" guarantee is not yet answered by it),
   `reference.go`'s `FieldRef` (partial match only — instance-level, not
   schema-structural like `atomic_ref`).
2. **A0.2 (stays in Relay)** — `loader.go`, the three `IaCSource`
   implementations (`source_file.go`/`source_git.go`/`source_upstream.go`),
   `validator.go` (Cabinet-registry graph resolution), and `core/nexus/drift`
   (a consumer of `iac.Evaluate`, not part of the semantics itself).
3. **A0.3 (real gaps, no `cic-primitives` counterpart)** — `sensitive.go`'s
   secret custody/placement policy (checked all 8 atoms: none model this),
   `observation.go`/`compare.go`'s coverage-and-comparison algorithm
   (section F territory, zero primitives-side vocabulary today), and
   `field_id.go`'s rename-survival guarantee (neither `identity.yaml`, which
   is type-level, nor `address.yaml`'s `logical_id`, which is instance-level,
   covers field-level addressing).
4. **A0.4 (actual conflicts, not just naming)** — conformance is tri-state
   in `cic-primitives` (`implemented`/`not_implemented`/`deprecated`) but
   boolean in Relay's `FieldMode.Implemented`, with **no representation of
   `deprecated` at all today**; `acl.go`'s POSIX-class algorithm is
   materially richer than the Access atom's flat OR-only `access`/`modify`
   lists; `behavior` names two unrelated things (Relay's `$cic.behavior`
   layer vs. the `Behavior` atom's rpc/action/operation definitions); and
   `default_injection` (Access atom) has no Relay counterpart at all — a
   denied read has no defined substitution mechanism in the Go model.

None of A–G are closed by this inventory. It replaces guessing with a
grounded starting point — in particular, section B cannot close without
resolving the tri-state/boolean conformance conflict, and section E has
partial Go prior art (`node.go`'s `Value`) worth reusing, but not yet a
full answer: `Node` itself is still an exported-field struct with an
opt-in `Validate()`, not a type that makes an unvalidated object
unrepresentable.

---

### A. Canonical representation

**Status:** **DECIDED** — adopts CIC's existing Canonical Object Encoding
(`core/nexus/iac`'s `number.go`/`canonicaljson.go`/`digest.go`) as
normative. Two gaps named, not resolved: Unicode normalization, and
`TopologySet` element canonical order (the existing Go code flags this one
itself, as a known placeholder). **Update:** the Rust side now
*implements* all of A1–A7 (`engine/src/canonical.rs`, PR #19) — the
"not yet implemented" note this line originally carried is stale.
A review on that PR also found and closed a real gap: `saphyr` was
silently demoting an integer literal beyond `i64` range to a lossy
`f64` before canonicalization ever saw it, fixed in `reader.rs` with a
new `Value::BigInt`. See `MATERIALIZATION-SPEC.md`'s A8 for the full
account.
**Blocks:** everything below — B through G all assume a canonical form exists
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#a--canonical-representation-closes-section-a`

Format: canonical JSON, no inter-token whitespace. Object keys sorted by
raw UTF-8 byte order (Go's `sort.Strings`, which a Rust `Vec<String>::sort`
reproduces identically — no new algorithm needed here, only a port).
Numbers per `number.go`'s proven, byte-identical-with-Rust rules (exact
integer digits, `-0`→`0`, shortest plain-decimal floats, normalized before
structural writing, never inline). Strings escape `<`, `>`, `&`, U+2028 and U+2029
as the literal six-character ASCII sequences `\u003c`, `\u003e`, `\u0026`,
`\u2028` and `\u2029` respectively (see `MATERIALIZATION-SPEC.md` for the
full table) — **verified empirically
in this session**, not assumed: this is Go's default `json.Marshal`
behavior (inherited by calling it directly), not an RFC 8259 minimum, and
a Rust writer must replicate it exactly or it will not produce
byte-identical output for any string containing those characters. No
Unicode normalization is applied anywhere in the existing pipeline —
confirmed empirically that a precomposed and a decomposed form of the same
visual string digest to different bytes; this is named as a known,
inherited limitation, not fixed here. Digest = `sha256:` + lowercase-hex
SHA-256 of the canonical bytes directly, no further transform.

See `docs/MATERIALIZATION-SPEC.md` for the full write-up, the empirical
verification detail, and exactly what remains open (Unicode normalization
and `TopologySet` order — the Rust peer itself is now written for all of
A1–A7, `engine/src/canonical.rs`, PR #19; not yet wired into a pipeline
stage, since `Parse`/`Normalize`/`Resolve`/`Validate` don't exist yet).

---

### B. Semantic state model

**Status:** **CLOSED.** B1 (axis model)/B2 (`missing`/`unknown`
resolved)/B3 (type shape)/B4 (A0.4 conformance resolution)/B5
(two-trigger `default_injection`) are all settled. **B7 is a
later-pass addition**, added while closing F1: section A's own A8 had
deferred a question to this section — whether capability/coverage/
provenance are part of the materialized semantic output at all — that
B's original closure never actually answered. B7 closes it: yes, as
semantic membership and canonical commitment (covered by
`output_digest`), not as a wire-layout decision.
**Blocks:** nothing further — unblocks full closure of C, E, and now
F1's coverage-projection question and C3's `unresolved/unknown
markers` row
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#b--semantic-state-model-closed`

`docs/BOUNDARY.md`'s "five distinct statements" are not five values of
one enum — they're points on **three separate, orthogonal axes**:
**capability** (D-012's `implemented`/`not_implemented`/`deprecated`,
static per device binding), **coverage** (`observed`/`absent`/
`unobserved`/`unknown`, dynamic per observe call), and **provenance**
(`authored`/`schema_default`/`derived`).

**B2, closed — both terms resolved, and the "fourth axis" question
answered no:**
- `missing` ≡ coverage's `absent`. Closed not by repeating the earlier,
  correctly-rejected assertion, but by cross-reading
  `docs/PRIMITIVE-IR.md`'s own, independent "Complete" property, which
  enumerates **three** absence-reasons (`authored-absent`,
  `not-observed`, `not-implemented`) — no fourth slot for "missing"
  either. The simplest reading: `PRIMITIVE-IR.md`'s single
  "not-observed" is the coarse union of what `BOUNDARY.md` splits finer
  into `not_observed` (never looked) and `missing` (looked, confirmed
  gone) — exactly `core/nexus/iac`'s existing `unobserved`/`absent`
  split, needing no new concept. **`PRIMITIVE-IR.md`'s `authored-absent`
  itself is NOT settled here** — review caught that an earlier draft
  wrongly equated it with an authored literal `null`, which conflates a
  presence statement ("the authoring side explicitly establishes
  absence") with a value statement (a schema-legitimate `null` *value*).
  Its representation is a separate, genuinely open question for
  section A, named but not resolved by B2 — `authored-absent` is not
  one of `BOUNDARY.md`'s five terms this section is actually scoped to.
- `unknown` is adopted as a new, fourth **coverage** value (for a
  device-reported indeterminate value, distinct from an affirmed
  absence) — the previously-offered candidate, finalized since no
  counter-reading surfaced while closing `missing`.
- **Three axes is the final count** — both terms resolved as coverage
  values, not as a reason to add a fourth axis. B1 is fully settled.

Also resolves A0.4's tri-state/boolean conformance conflict: the library
carries the full tri-state; Relay's existing `FieldMode.Implemented bool`
becomes a named **lossy** projection of it until Relay migrates (step 6).

Clarifies `default_injection` as **two independent triggers, not one** —
review caught that modeling it as pure ACL-denial erodes D-012's own
"permission denied ≠ capability missing" distinction: a `not_implemented`
read returns `default_injection` regardless of ACL (no permission check
even applies), a permission-denied read returns it separately, and the
two diverge sharply on write (hard reject vs. permission denied) in a way
that must never be collapsed. `MaterializedField.value` itself is
**optional**, not "always present" — review also caught that this
can't be true for `coverage: absent/unobserved` or
`capability: not_implemented`, which are definitionally cases with
nothing real to hold.

Also names a real risk and heads it off: `cic-schema-registry`'s
`coverage.py` (this session's earlier D-017 work) already uses the word
`missing` for an unrelated, already-decided concept — a schema-evolution
violation (a field absent across schema *versions*), not an instance's
observed-object state. Called out explicitly so the two don't get
conflated the way `canonicalNumber`/`normalizeNumbers` did in section A.

**B7, closed later — capability/coverage/provenance are part of the
materialized semantic claim, not purely in-memory/engine-internal
facts.** This closes a forward reference A8 made that B's original
closure never answered: whether these three facts are part of the
canonical output at all. Two already-closed decisions force the
answer, taken together: B3 already says a module can inspect status
*independently* of value, and `BOUNDARY.md` already says the type
system does not survive serialization — a module across a real
process boundary can only inspect status that way if it is actual
*data* that crossed the wire, not a type-level annotation. So: a
change in capability, coverage or provenance is a change in the
materialized claim, and `output_digest` (A7) must cover it — same
`value`, different `coverage` or `provenance`, must not digest
identically. **This decides semantic membership and canonical
commitment only, not wire layout** — the physical nesting of these
facts next to `value` stays open, for C4/E2b, exactly where A8 already
left layout questions. This also closes F1's coverage-projection
question (see section F) and C3's `unresolved/unknown markers` row
(see section C): coverage no longer needs the receipt to survive the
wire, because B7 already commits it via `output_digest`.

---

### C. Receipt schema

**Status:** **CLOSED.** C1 (sibling artifact, not
IR-embedded), C2 (produced every materialization call), C3's
version-identity row (filled in by D1's four identifier groups, once D
closed — a later-pass fill-in, not decided when this table was first
written), and C5 in full are settled. C5 closes both its
*classification* rule (`provenance` alone decides whether an
`applied_defaults`/`derived_values` entry exists — no second source
needed for that yes/no) **and** its entry *evidence*: `rule`/
`value_digest` for a schema default (`DefaultEvidence`) and `rule`/
`inputs`/`value_digest` for a derivation (`DerivedEvidence`), produced
inline by the same materialization step that applies the default or
runs the derivation — a C-owned companion fact, never folded into B3's
closed `provenance` enum. `inputs` records the exact paths that
specific invocation actually read, not a static property declared by
the rule's own definition; `value_digest` is present on both evidence
shapes so a `derived_values` entry is auditable on its own, the same
way an `applied_defaults` entry already is. C3's `signature` row is
also now closed **as to whether this engine signs**: this engine's own
README excludes Vault access and counter-signature policy from its
scope, so this engine does not sign and does not decide signer
authority. **Correction (review-caught on PR #14):** a first pass
over-reached that into "this receipt schema carries no `signature`
field," which `cic-primitives`' own D-015 actually argues against —
its release bundle's schema *does* define a `release.sign` field,
populated by a separate, external `_vault_sign()` call rather than by
the build logic itself. Schema ownership and producer ownership are
different facts, so whether this receipt's schema ever reserves a
slot for an externally-produced signature is left as C4's own
schema-layout question, not decided here. C3's `unresolved/unknown
markers` row is also now closed, by B7 (added later, while closing
F1): coverage, including `unknown`, is already part of the
materialized semantic claim `output_digest` commits to, so this row
needs no field of its own — the custody question is settled, not
merely unblocked. **C4's WHICH-REPO question is also now closed**
(added later, while closing C4): the schema lives in this repo, not a
new or separate location — see C4's own entry below. **C3's
`conformance_plan_digest`/`observation_digest` rows are also now
closed, by F5** (added later, while closing F): neither is a receipt
field at all — both belong to the conformance/drift verdict artifact
(F3), with their own scope and byte-level semantics decided there.
**C4's core field layout is also now closed** (added later, while
writing and then, on review, tightening it): `docs/RECEIPT-SCHEMA.md`
lays out every field, cited to its source decision, every field
REQUIRED, the two evidence arrays sorted by `.path` byte-wise and
required-but-possibly-`[]`, `derived_values[].inputs` a deduplicated
sorted set. "Schema text: CLOSED" is corrected to "core field layout:
CLOSED." **And, closing section C in full: the three extension/meta
fields this left open are now all decided, directly in
`docs/RECEIPT-SCHEMA.md`** — `receipt_schema_version` added (decided
yes, extending `PRIMITIVE-IR.md`'s Versioned property); no
signature-related field (decided no, extending C1's own
sibling-not-embedded reasoning one level further — a slot for the
receipt's own eventual signature would be a smaller version of the
exact regress C1 avoids); no redundant coverage projection (decided
no, on this document's own "a field with no citation does not belong
here" discipline — F1/F4's custody question was already settled by
B7). **Section C is CLOSED.**
**Blocks:** nothing of its own any more — differential conformance (G)
and proof-chain integration still wait on F's comparator walk, not on
anything in C
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#c--receipt-schema-closed`

1. **Closed:** sibling artifact bound by digest, not IR-embedded — this
   is a *recovered* decision: `BOUNDARY.md` already settled it, with its
   own stated reason (avoiding the archived model's infinite provenance
   regress). `PRIMITIVE-IR.md` open question #1 is answered.
2. **Closed, re-grounded after review:** produced on every materialization
   call, not only at release. An earlier version justified this by
   claiming it matches already-landed Go behavior (`core/nexus/iac`'s
   `proof` computed per `Evaluate()` call) — **wrong**: `Evaluate()`
   returns a `ConformanceResult` (an intent/state drift verdict), not a
   materialization receipt, and the `proof` index A0 pointed to is itself
   only *sketched*, explicitly named as **deferred** in
   `iac-object-model.md`. The decision stands, but on its actual ground:
   `BOUNDARY.md`'s own custody-boundary logic (*"the materialization
   receipt... is the only part of the boundary that survives the
   wire"*) requires a receipt for every materialization, or runtime
   materializations between releases cross the module boundary with no
   surviving evidence custody held.
3. **Partially decided, re-grounded after review, and partly closed in
   a later pass:** the minimum field set reconciles `BOUNDARY.md`'s
   sketch with what's actually proven. `BOUNDARY.md`'s own fields are
   decided: the two digests and `applied_defaults`/`derived_values`
   (derived from B3, see item 5 below). **Now also closed, as to WHICH
   facts, not their wire layout:** the version-identity row
   (`grammar_version`/`primitive_release`/`schema_version`/validator
   identity) — D1's four identifier groups fill in *which logical
   facts* the receipt must carry (grammar digest, primitive release
   identity, domain schema identity, validator/engine identity).
   **Correction (review-caught):** an earlier version called this "the
   receipt's version block," implying a single nested object — a
   specific wire representation D never decided. Whether these four
   facts nest under one key, sit as flat top-level fields, or
   something else is **C4's call**, same as every other field's
   placement, not settled by D1 or by this row closing.
   **`signature` is now CLOSED, as to whether this engine signs — but
   narrower than a first pass over this claimed.** An earlier version
   claimed "Relay already signs this," which was false (no landed
   precedent exists; the sketched `proof` index that would carry it is
   itself deferred) — that correction stood, but left the row merely
   "nothing grounds it yet." It closes properly on this engine's own
   boundary instead: the README's "what does not belong here" names
   Vault access and counter-signature policy as explicitly excluded.
   **Decision: this engine does not sign, and does not decide signer
   authority.**
   **Correction (review-caught on PR #14): that is not the same as
   "this receipt schema can never carry a signature-related field."**
   A pass closing this row that way cited `cic-primitives`' D-015 as
   support, but D-015 actually argues the opposite: its release
   bundle's own schema *does* define a `release.sign` field — it is
   simply populated by a separate, external `_vault_sign()` call (by a
   different authority, after `build_hash`), never by the compiler's
   own build logic (`tools/compiler.py`). **Schema ownership and
   producer ownership are different facts.** Whether this receipt's
   own schema ever reserves a slot for an externally-produced
   signature (parallel to `release.sign`), or a signed form is always
   a wholly separate sibling artifact wrapping this receipt, is **not
   decided here** — that is a receipt *schema layout* question,
   squarely C4's territory, the same way D1's version-identity facts
   were kept separate from their wire layout two paragraphs up.
   Deciding it now would repeat that exact boundary violation one row
   over. A second, smaller overclaim is also narrowed here: producing
   a signature requires an external signer/key authority (Vault
   access) — it does **not** also require a counter-signature policy;
   counter-signing (`cic_countersign`, per D-015: "applied after
   signing, by a different authority") is a separate, optional second
   layer, not a precondition for a first signature.
   `unresolved/unknown markers` was reserved pending B2
   (`missing`/`unknown`); B2 closed with a real shape for both terms
   (`missing` ≡ coverage's `absent`, `unknown` a new fourth coverage
   value) — but that only unblocked the *value*. Whether coverage
   projects into the receipt at all was then correctly left as F's own
   open question (F1/F4), not C3's to decide — deciding it from here
   would have meant quietly answering F's question from inside C, the
   same kind of boundary violation this file has caught and corrected
   before (F1 regressing B3, F3 re-asserting coverage in the receipt).
   **This row is now CLOSED, by B7** (added later, while closing F1):
   capability/coverage/provenance are part of the materialized
   semantic claim `output_digest` already commits to, so coverage
   doesn't need a receipt field to survive the module boundary — the
   row closes with no field, the custody question fully answered, not
   merely unblocked. **`conformance_plan_digest`/`observation_digest`
   are now CLOSED too, by F5 — see F's own entry below for the
   semantics.** Neither is a receipt field: `conformance_plan_digest`
   is section F's territory with no landed precedent at all
   (`ConformancePlan` exists in `conformance.go` but is never digested
   there); `observation_digest` **is** landed, but only as a field of
   `ConformanceResult` (the drift verdict, digesting the coverage
   envelope alone, not observed values) — a different artifact from
   this receipt, never simply reusable here, and F5 deliberately widens
   its scope for this engine's own version of it.
4. **CLOSED as to WHICH repo; the schema's own text stays deferred.**
   The schema does **not** live in `cic-primitives`' `schemas/atomic/`/
   `schemas/aggregate/` — giving the receipt a primitive's schema home
   would reintroduce the exact category error `BOUNDARY.md`'s regress
   argument (item 1) exists to avoid. This item originally deferred the
   *whole* "which repo, or a new location" question until item 3's
   field list was complete, reasoning that schema-ing a partially-known
   field set would bake in gaps. That reasoning is sound for *writing
   the schema's actual text*, but not for *deciding which repository
   hosts it* — a coarser, organizational fact that doesn't depend on
   knowing every field, the same kind of split D1 drew between which
   version-identity facts must exist and how they're nested on the
   wire. **Decided: the schema lives in this repo
   (`cic-primitive-engine`), not a new or separate location** — three
   convergent reasons: (a) A0's closed meta-decision that this library
   is the single semantic authority, not a contract split across
   repos — the strongest of the three on its own. (b) **Corrected
   (review-caught on PR #18):** an earlier pass argued the
   already-closed `signature` row means the receipt "needs none of" the
   signed, Vault-backed release pipeline that justified splitting
   `cic-primitives` out — that overreaches past what item 5 actually
   closed (only that *this engine* doesn't sign and doesn't decide
   signer authority; whether the receipt's schema reserves a slot for
   an externally-populated signature is still open, C4's own call).
   The sound version doesn't depend on that open question at all:
   semantic ownership (who defines the receipt's fields) and signature
   *production* (who, if anyone, signs it) are orthogonal — whichever
   way the signature-slot question resolves, external signing never
   transfers ownership of the receipt's semantic schema to whoever
   signs it, the same way `cic-primitives`' schema isn't owned by Vault
   just because Vault signs its releases. (c) **Corrected:**
   `docs/PRIMITIVE-IR.md` is precedent for this repo owning the
   *contract* for an artifact it produces, not for "a formal schema
   already lives here" — the file itself opens *"Not specified yet..."*,
   a required-properties document, not a working schema; no formal
   schema exists yet for either artifact. The receipt is its sibling
   per item 1, produced by the identical pipeline — narrower precedent
   than first stated, but real.

   **Update: the schema's actual field-by-field text has since been
   written** (`docs/RECEIPT-SCHEMA.md`) — every field cited to its
   source decision, version-identity facts laid out flat (one key per
   D1 identifier, not nested, since D1 established they vary
   independently), signature and coverage-projection fields
   deliberately omitted (still open, not decided by writing the
   layout). Writing it also surfaced a latent overlap this tracker
   never named: C3's `schema.digest` row (from `BOUNDARY.md`'s pre-D1
   sketch) and D1.3's richer three-part domain-schema-identity group
   were never reconciled as the same slot — now one field
   (`schema_digest`), not two. Also surfaced, and left open rather than
   invented: whether each receipt INSTANCE needs its own schema-version
   field.

   **Correction (review-caught on PR #24): a field-name/type list is
   not a byte-level contract — the same gap F5 had to fix one section
   up, recurring here.** Nothing in the first version said whether
   `applied_defaults`/`derived_values` may be omitted when empty, or
   what order their entries (or `derived_values[].inputs`) appear in —
   and section A's A6 deliberately doesn't sort `Seq`s, so two
   conforming implementations could emit different bytes for the same
   semantic receipt. Fixed: every field is now REQUIRED (extending
   `PRIMITIVE-IR.md`'s Complete property to this sibling artifact), the
   two evidence arrays are required-but-possibly-`[]`, both sort by
   `.path` byte-wise, and `derived_values[].inputs` is decided — newly,
   not inherited from C5 — to be a deduplicated sorted path set, not an
   execution-order trace. **Also corrected: calling this whole item
   "CLOSED" overstated it** — the *core field layout* is closed; three
   extension/meta questions (signature slot, coverage projection,
   receipt-instance schema-version) stay open, same distinction this
   file already draws elsewhere between what a closure settles and
   what it doesn't.

   **Update: the three extension/meta questions have since closed too,
   directly in `docs/RECEIPT-SCHEMA.md`, completing section C.**
   `receipt_schema_version` added (decided yes, extending
   `PRIMITIVE-IR.md`'s Versioned property); no signature-related field
   (decided no, extending C1's own sibling-not-embedded reasoning one
   level further); no redundant coverage projection (decided no, per
   this document's own "no citation, no field" discipline — F1/F4's
   custody question was already settled by B7). **Still genuinely
   open:** only the exact in-repo path (trivial, non-blocking — it
   landed at `docs/RECEIPT-SCHEMA.md`, not a claim no other path was
   possible).
5. **CLOSED — classification and evidence, both specified; review
   caught an overclaim here too, on the way to closing it.** B3's
   `provenance` was first wrongly scoped "intent/input side only,"
   which couldn't produce `BOUNDARY.md`'s own state-side
   `derived_values` example (`$.state.effective_state`, derived from
   `$.state.admin_state`/`$.state.oper_state`). Fixed: `authored`/
   `schema_default` stay intent-side only (the latter also
   independently forbidden state-side by B1's defaultability rules), but
   `derived` can occur on either side — a state field can carry both
   `coverage: observed` and `provenance: derived` at once. **Then a
   second overclaim**, in C5 itself: saying the receipt's
   `applied_defaults`/`derived_values` entries are "computed from
   `provenance`" implied the enum alone is sufficient. It isn't —
   `provenance: derived` says *that* a field was derived, not
   `BOUNDARY.md`'s required `rule`/`inputs` (or, for `applied_defaults`,
   `rule`/`value_digest`), which the enum has no room for. Corrected to
   two distinct things: `provenance` is the **authoritative
   classification** (decides list membership, avoiding a second source
   for that fact — the real two-sources-of-truth risk A0 cares about),
   while the entry's **evidence payload** is now specified as a
   C-owned companion fact, produced inline by the same materialization
   step that already has to know it to do its own job:
   `DefaultEvidence{rule, value_digest}` when Normalize applies a
   schema default (`rule` a literal, unversioned `"schema-default"`
   constant — the corpus shows exactly one default-application
   algorithm, not a family of named rules), `DerivedEvidence{rule,
   inputs, value_digest}` when a derivation step runs (`rule` a stable,
   versioned string per `BOUNDARY.md`'s own `effective-state-v1`
   example). **A third overclaim, caught on review of the PR that
   closed this item:** the first draft defined `inputs` as a *static*
   list declared by the rule's own definition, and gave `DerivedEvidence`
   no `value_digest` at all (asymmetric with `DefaultEvidence`).
   Neither survives — `BOUNDARY.md`'s single example happens to read a
   fixed pair of paths every time, but that is a property of that one
   rule, not a constraint on every future rule (a selector-based
   aggregation's consumed set can vary by invocation); corrected so
   `inputs` always records the exact paths THIS invocation actually
   read, which also still matches the one existing example, exactly.
   `value_digest` was added to `DerivedEvidence` so a single
   `derived_values` entry is auditable on its own — the receipt's
   top-level `output_digest` already binds the whole canonical output,
   but cannot verify one entry without re-canonicalizing and extracting
   that path from the full document, the same gap a per-entry digest
   closes for `applied_defaults` already. Neither shape modifies B3's
   already-closed `provenance` enum — the evidence sits beside the
   classification, the same way C1 keeps the receipt beside the
   materialized data rather than embedding it. The receipt is a
   deterministic projection of both: `provenance` decides which list an
   entry belongs to, the matching `*Evidence` struct supplies that
   entry's payload.

The receipt is not a byproduct. If the CIC proof chain is to mean anything
here, the materialization receipt **is** the evidence that the custody
boundary (section E) held — not optional metadata beside it.

---

### D. Version binding

**Status:** **DECIDED**
**Blocks:** differential conformance (G) — now unblocked; C3's reserved
version-identity fields — now filled
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#d--version-binding-closes-section-d`

`PRIMITIVE-IR.md`'s "one number or two?" framing (open question #4) is
answered **neither** — there are **four** independently varying
identifiers, not one or two:

1. **Grammar digest** — `grammar_sha256`/`grammar_schema_sha256`, file
   content hashes of the atom-grammar checker and its schema, not a
   semver tag alone. Adopted by name from `cic-primitives`' own,
   already-landed D-015 envelope-v2 provenance block
   (`tools/compiler.py`'s `_collect_provenance()`, verified directly in
   source — run on every real release, not a sketch).
2. **Primitive release identity** — the signed release tag
   (`primitives/@v0.2.0`) plus its own `source_commit`/`build_hash`;
   distinct from (1) because the rules a release enforces and the
   release artifact itself are different facts.
3. **Domain schema identity** — not a bare version number (review caught
   this inconsistency: D2 requires "identity + version/digest," so the
   group has to actually contain those). Three parts, one group: logical
   identity/canonical name, version, and a canonical content digest —
   for the same reason item 1 isn't a bare tag, a version string alone
   isn't an immutable identity. Varies per schema, independently of the
   grammar.
4. **Validator/engine identity** — which implementation (Go vs. Rust)
   and its own version produced this materialization — required so
   section G's differential conformance can tell the two sides apart at
   all. **This is also the one identifier that is supposed to differ**
   between the two sides of a differential test — forward note for
   section G: "compare the receipts" has to mean three separate
   equality questions (materialized-value bytes, receipt semantics
   excluding this field, and this field itself), not one byte-equality
   check.

**D2, corrected after review:** grammar digest alone is **not**
sufficient as the comparability gate — an earlier version of this
section gated only on grammar digest match, which review caught misses
that items 2 and 3 above vary *independently* of the grammar: two
implementations could share an identical grammar digest while
materializing against different primitive releases or different domain
schema versions, and any byte difference between them would reflect
different semantic input, not implementation divergence — backwards
from what section G exists to measure. Corrected: the Go and Rust
libraries still release independently, not in lockstep, but **every**
semantic-input identifier must match before comparison — grammar digest,
primitive release identity, schema version/digest, and the authored
input digest (section A) — with validator/engine identity explicitly
excluded from the gate, since it's supposed to differ. Any gate mismatch
is `NOT COMPARABLE`, never reported as a found divergence.

**Concrete, immediate consequence (not done by this section, flagged as
follow-up):** this engine's own `dependency.yaml` has tracked
`cic-primitives` at unpinned `main` since 2026-08-13, specifically
because the anticipated grammar wasn't in any release tag yet.
`primitives/@v0.2.0` now contains exactly that grammar (confirmed earlier
this session). `dependency.yaml`'s own stated closing condition is now
satisfied — pinning it is a mechanical follow-up with no remaining
judgment call, out of scope for this docs-only change.

---

### E. Boundary enforcement

**Status:** **PARTIALLY DECIDED, not closed.** E1, E2a and E3 are
settled. E2b (exact type shape, receipt-transport wire shape) is open.
Whether native and WASM modules *should* get the *same* trust/policy
treatment inside the mandatory boundary (a different question from E1
— review caught an earlier draft conflating the two) is open as a
normative question, though now confirmed, by reading `NativeImpl`'s own
call-shape check, that they don't even share a mechanism *today*
(native has no identity-carrying parameter at all; WASM has a
meaningless one). How a real actor identity gets threaded through was
a genuine gap found in the live code — now checked directly against
it: `iac.Actor{...}` has zero production construction sites anywhere
in the repo, and `SetPayload` (the external API's own entry point)
carries no identity field either, so nothing existing is available to
wire in; designing the real mechanism remains open. The guest-side
digest-check shape is open.
**Blocks:** F, G — nothing downstream is meaningful if a module can
bypass materialization
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#e--boundary-enforcement-partially-decided-not-closed`

1. **Closed, grounded in the live codebase, not designed from scratch:**
   `CIC-Relay/core/cabinet/service.go`'s workflow step executor is
   *already* the single chokepoint every module call passes through —
   native Go and WASM alike, **mandatorily — neither bypasses it.**
   **Correction (review-caught):** an earlier draft's "one gate covers
   both paths" conflated that structural fact (where the boundary sits)
   with a claim about trust/policy treatment *inside* it (what happens
   once past it) — the latter is explicitly **not** decided here (see
   below); E1 only fixes that neither path routes around the chokepoint
   itself. Verified what runs there today: `validateInputSchema` only
   checks a `"$schema"` key's presence/membership (a routing guard, not
   materialization), and `hashValue` is section A's exact canonical-digest
   pipeline (`canonicaljson.ToJSON` → SHA-256) — but feeding
   **ProofTrace's own chain-of-custody**, a different proof artifact
   from this effort's materialization receipt (section C); reusing the
   digest code is fine, conflating the two artifacts is not.
2. **Split, after review caught this section marked itself both open
   and closed in different places (the same mistake already fixed once
   in section B):**
   - **E2a, closed:** that chokepoint must accept only a
     constructor-gated handle, never a raw `inputData interface{}` —
     host and resolver are the same process at that point, so Go's type
     system genuinely can enforce this (the in-process case
     `BOUNDARY.md` already said was achievable, not yet built).
   - **E2b, open:** the type's exact Go representation, and the
     receipt-transport wire shape (`service.go`'s current
     `Process(ctx, authContextJson, inputJson string)` only has room for
     two strings — not designed here).
   **A real, found gap, checked, still open:** `authContextJson :=
   step.ComponentID` is the calling component's own ID, not an actual
   actor identity `acl.go`'s `ACL.Allows` could evaluate — so B5's
   capability/ACL gates can't run meaningfully at this point *yet*.
   This directly answers what B5 left open (what prevents a module from
   reading a value it isn't entitled to): this same gate, once a real
   identity is threaded through — not a separate mechanism. **This
   section originally left "whether Relay already has one available, or
   needs one built" unchecked — now checked, against the code:**
   grepping every `.go` file for `iac.Actor{` construction found zero
   production call sites (only `acl_test.go`'s own unit tests), and
   `SetPayload` (`core/cabinet/set_schema.go`, the external API's own
   request struct — the top of this whole call chain) carries
   `WorkflowID`/`Modules`/`Payload`/`Options`/`SourceDigest` and nothing
   resembling an identity. **Decided: this needs building, not
   finding** — no existing Relay mechanism is sitting nearby to wire in.
   Designing it stays open; the search for an existing one is now done
   and came back empty, not merely undone.
   **Also open, distinct from item 1's structural claim:** whether
   `NativeImpl` (first-party, non-WASM) modules get the *same*
   capability/ACL treatment as `WasmCode` modules once past the
   mandatory chokepoint, or a different trust tier there. "Same
   chokepoint for both" (item 1, closed) is not the same claim as "same
   policy for both" (open) — and checking `NativeImpl`'s own call-shape
   verification in `service.go` shows they don't even have the same
   *mechanism* yet: its call signature has no slot for an identity
   argument at all, where `WasmCode`'s `authContextJson` is at least
   present, if meaningless. A found asymmetry, not a normative answer.
3. **Closed:** the gate runs entirely before any WASM boundary is
   crossed, in Go, regardless of which language the eventual guest was
   compiled from — identical for a Go guest and a Rust guest. The one
   language-specific piece is defense-in-depth: a guest independently
   verifying `digest(received_bytes) == receipt.output_digest` on its
   own side — cheap, needed once per guest language, not designed
   further here.

---

### F. Output symmetry

**Status:** **PARTIALLY DECIDED, not closed.** F1's model claim (B's
existing coverage/provenance axes already cover output — corrected after
review caught a regression to an input-vs-output framing B3 was already
fixed away from), F1's custody claim (coverage needs no receipt
projection to survive the module boundary — closed by B7, added later:
coverage is already part of the materialized semantic claim
`output_digest` commits to), F2 (the comparator is migration source
under A0's existing meta-decision, made explicit for this axis), F5
(`conformance_plan_digest`'s and `observation_digest`'s scope and
byte-level semantics — both are conformance/drift-verdict-artifact
fields, never receipt fields; see item 3 below), and F6 (the per-field
comparator PRIMITIVE — `compare`/`classify_field`/
`classify_field_value` — implemented in Rust, `engine/src/
conformance.rs`, ported from `compare.go`/`observation.go` and tested
against that file's own vectors; see item 4), F7 (the collection-
topology/element-identity PRIMITIVE — `Collection`/
`CollectionTopology`/`element_key`, full `fmt.Sprintf("%v", ...)`
parity including `Float`/`Seq`/`Map` — implemented in Rust,
`engine/src/collection.rs`, ported from `collection.go` and tested
against that file's own vectors; see item 5), the redundant-coverage-projection
question (CLOSED, decided no, in `docs/RECEIPT-SCHEMA.md` while
closing section C in full), and F8 (the object-level comparator/
verdict WALKER — `ConformancePlan`/`FieldPlan`/`CollectionPlan`/
`evaluate`/`aggregate`/`elementKeys`/`resolvePath` — implemented in
Rust, `engine/src/plan.rs`, ported from `conformance.go` and tested
against all seven of that file's own end-to-end OCI vectors; see item
6), and F9 (F5's `conformance_plan_digest`/`observation_digest`
projections WIRED to the real `ConformancePlan`/consumed-observation
types F8 introduces — `engine/src/digest_projection.rs`, see item 7),
and F10 (the conformance-verdict artifact's own wire layout —
`docs/VERDICT-SCHEMA.md` — AND the procedural relationship between
the verdict, the receipt, and ProofTrace, decided independent; see
item 8) are settled. **Open:** Go's analogous `IntentDigest` still has
no equivalent here (it needs a `SpecDigest`-style expand+normalize
step this engine's `Normalize` stage doesn't have yet, so neither F9
nor F10 invents one), and whether the verdict artifact is itself ever
signed (`VERDICT-SCHEMA.md`'s own explicit non-decision, not inherited
from the receipt's closed "no signature field").
**Blocks:** proof-chain completeness
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#f--output-symmetry-partially-decided-not-closed`

```text
module observation
   ↓
normalize
   ↓
validate
   ↓
canonicalize
   ↓
receipt/evidence
   ↓
proof chain
```

1. **Closed, recovered from `BOUNDARY.md`, not newly decided —
   corrected after review caught two overclaims in this item's first
   draft:** `BOUNDARY.md`'s own diagram already specifies the output
   path (`ValidatedMaterializedInput → module → UntrustedModuleOutput →
   output schema validation → ValidatedObservation/ValidatedConsequence`)
   and its principle (*"a module's output does not inherit trust from
   its input"*). The planned resolver's *model* already covers it
   without new scope — but **not** via an input-vs-output split. An
   earlier draft said "coverage for output, provenance for input, never
   both," which regressed exactly the oversimplification B3 was already
   corrected away from: a derived state field legitimately carries
   **both** `coverage: observed` and `provenance: derived` at once
   (`BOUNDARY.md`'s own `effective_state` example). The real split is
   `coverage` = observation knowledge, `provenance` = value origin — not
   which direction the data came from. Separately, section A's canonical
   form has no input/output distinction at all. **Section C's receipt
   mechanism is direction-agnostic (C2's every-call production applies
   either way) — but this does not mean the receipt carries a coverage
   payload.** C3's actually-decided fields are digests plus
   `applied_defaults[]`/`derived_values[]`; no coverage field exists
   there today — **and, per B7 (added later, while closing this very
   item), none is needed: coverage doesn't require the receipt to
   survive the module boundary, because it's already part of the
   materialized semantic output `output_digest` commits to.**
   `conformance_plan_digest`/`observation_digest` were, at the time
   this item was written, exactly as open as C3 said — closed since,
   by F5 (item 3 below). What doesn't exist yet, in
   either language, is the actual comparator/verdict-aggregation logic —
   today that's Relay's own Go code
   (`compare.go`/`observation.go`/`conformance.go`), landed and tested
   but scoped to one OCI vertical slice with no Rust peer.
2. **Closed, making an existing decision explicit, not re-deciding it:**
   A0's meta-decision already settled that `core/nexus/iac` is migration
   source, not a permanent second contract, generally. This section
   states plainly that `compare.go`/`observation.go`/`conformance.go`
   fall under that same decision specifically — they do not stay separate
   indefinitely; they get absorbed per the roadmap's step 6, same as the
   rest of `core/nexus/iac`'s inventory. Leaving this look undecided for
   one specific piece of code, when the general principle was already
   settled, would itself be the kind of silent-divergence risk this
   question named (cf. D-017's explicit scoping of `deprecated`) — so
   it's answered here, not left open by omission.
3. **Closed — `conformance_plan_digest`'s and `observation_digest`'s
   scope AND an exact, named preimage shape for each, grounded by
   reading `conformance.go`/`observation.go`/`compare.go`/
   `collection.go` directly rather than assuming the names imply
   existing behavior.** Both are fields of the conformance/drift
   verdict artifact (artifact 3 below) — never the materialization
   receipt (artifact 2). `conformance_plan_digest` has **no landed
   precedent at all** (`ConformancePlan` is never digested in
   `conformance.go`); `observation_digest` **does** have landed
   precedent, but narrower than the name suggests — it digests only
   `Observation{Observed, AuthoritativeAbsent}` (the coverage
   envelope), never the compared values — and this engine's version
   deliberately widens past it, for the same reason B7 widened the
   materialized tree: a semantic claim's constituent facts must be
   committed as real, digested data, not merely present at evaluation
   time.
   **Correction (review-caught on PR #23): naming the semantic
   content is scope, not a byte-level contract** — two
   implementations can both honestly satisfy the same semantic
   description and still produce different trees, hence different
   digests. Fixed with two named, exact `Value`-tree shapes
   (`docs/MATERIALIZATION-SPEC.md`'s F5, in full, including every
   array's ordering rule — section A sorts `Map` keys but has no
   opinion on `Seq` order, so the projection supplies its own
   byte-wise sort by `path`, reusing A2's comparator):
   - **`PlanDigestProjection`** — `{scalars: [{path, compare}],
     collections: [{path, topology, keys, elements: [{path,
     compare}]}]}`, `compare` being the literal `CompareType` string
     (`compare.go`), `topology`/`keys` being `Collection`'s own three
     topology values and key-field list (`collection.go`).
   - **`ObservationDigestProjection`** — `{fields: [{path, coverage,
     value?}]}`, `coverage` being **B3's actual four values**
     (`observed`/`absent`/`unobserved`/`unknown`), not Relay's
     two-list envelope — the second half of the review's catch: a
     digest that can't distinguish `unknown` from `unobserved` doesn't
     commit to the distinction B2/B7 exist to make. `value` is present
     **iff** `coverage == observed`, grounded in `ClassifyFieldValue`'s
     own logic (only `CoverageObserved` ever reads a value; every
     other state says "the comparison is irrelevant — coverage alone
     decides"), not in `MaterializedField`'s separate, already-closed
     value-presence rule (a different artifact's question).

   This does not implement the comparator (F1's gap stands) and does
   not decide the verdict artifact's own wire layout (no schema-layout
   decision exists for it yet, same separation as C3/C4) — only what
   each digest's bytes are taken over, at the same byte-level precision
   section A already holds materialization to.
4. **Closed — the per-field comparator PRIMITIVE, implemented in Rust
   (`engine/src/conformance.rs`), narrowing F1's gap rather than closing
   it.** Ports `compare.go`'s `Compare` and `observation.go`'s
   `ClassifyField`/`ClassifyFieldValue` — not `conformance.go`'s
   object-level `ConformancePlan`/`Evaluate`/`aggregate` walk, which
   remains unimplemented in either language. Every test vector ported
   case-for-case from `compare_test.go`'s own
   `TestCompare_Exact`/`TestCompare_Numeric`/`TestClassifyFieldValue`.
   `Coverage` is B3's closed four-value enum, not Relay's two-list
   envelope — there is no object-level walker here to hold the
   two-list form.

   **A new decision, not a port, because Relay has nothing to port:**
   `unknown` (B2's own addition, no Relay equivalent) classifies the
   same as `unobserved` — a device-reported indeterminate value is not
   authoritative evidence either way. The narrowest extension of the
   existing rule, not a guess at a richer one (e.g. a dedicated
   `INDETERMINATE` verdict); revisable without touching this
   function's other branches.

   **A faithfully-ported, named asymmetry, verified empirically, not a
   bug:** the numeric comparator converts a decimal-string literal to
   an exact rational by parsing its digits directly (mirroring
   `big.Rat.SetString`), and converts an `f64` from its IEEE-754 bits
   directly (mirroring `SetFloat64`) — never through each other.
   `"0.1"` (string) parses to exactly `1/10`; the `f64` `0.1` converts
   to `3602879701896397/36028797018963968` — a different rational, in
   both Go and this port. `compare_test.go`'s own vectors never
   exercise this (they only use binary-exact values like `1.5`/`100`).
   Not fixed here — fixing it would diverge *from* the Go reference
   section G exists to differentially compare against.

   **Review-caught gap (PR #25), fixed before merge: the numeric
   string grammar was missing `SetString`'s fraction form ("a/b")
   entirely.** `compare.go`'s `ratFromString` wraps `SetString` with no
   narrowing of its own, so this port's grammar had to match it, not a
   plausible-looking decimal/scientific subset. Missing the fraction
   form meant `compare(Str("1/2"), Str("0.5"), Numeric)` disagreed with
   Go (`comparable=false` here vs. `comparable=true, matched=true`
   there) — exactly the divergence G exists to catch, caught by review
   instead, since G's own harness can't run yet (F1's object-level
   walker doesn't exist). Fixed by trying the fraction form first (no
   grammar overlap with the decimal form).

   **Review-caught gap, round two, fixed before merge: disclosing the
   remaining gap honestly wasn't enough.** Review rejected shipping a
   known, reachable Go/Rust divergence (`CompareNumeric("0x10", "16")`
   would have disagreed) just because it was rare — section G's whole
   purpose is differential equivalence, and the PR's own declared goal
   was "port `Compare`." Fixed by porting `SetString`'s complete
   grammar: binary/octal/hex integers and floats, each base's own
   `"e"` (×10, unavailable for hex) or `"p"` (×2, available on *every*
   base, including plain decimal — `"1p1"` → `2`) exponent, and
   digit-separating underscores, including the one-underscore-right-
   after-a-prefix exception. Every claim and every rejection verified
   against real Go output in a Docker container before being encoded.
5. **Closed — the collection-topology/element-identity PRIMITIVE,
   implemented in Rust (`engine/src/collection.rs`), narrowing F1's gap
   further rather than closing it.** Ports `collection.go`'s
   `Collection`/`CollectionTopology`/`ElementKey` — not `elementKeys`
   (the function that calls `ElementKey` over both the intent and
   observed sides and unions/sorts the results), `resolvePath`,
   `Evaluate`, or `aggregate`, all of which remain unimplemented.
   Every test vector ported case-for-case from `collection_test.go`'s
   own `TestCollection_ElementKey`.

   **Review-caught gap (PR #27), fixed before merge: refusing to format
   `Float`/`Seq`/`Map` values was narrower than Go for no principled
   reason.** The first version refused all three, reasoning that an
   identity should never be "guessed" (`BOUNDARY.md`'s defaultability
   table, `structural: key`: *"identity is never guessed"*) — sound for
   a map-topology *key field*, but not for a `TopologySet` element,
   where the whole value already **is** the identity by construction;
   there is nothing to guess, only something to format, exactly as Go
   does unconditionally. Review correctly placed this in the same
   category as item 4's rejected rarity-based narrowing (PR #25).

   **Fixed: full `%v` parity**, verified empirically in a Docker
   container. `float64`'s `%v` is `strconv.FormatFloat(f, 'g', -1,
   64)`: plain decimal for a leading-digit decimal exponent of
   `-4..=5`, scientific otherwise — the threshold held regardless of
   significant-digit count, across a sweep from exponent -8 to 25, a
   different algorithm from section A's own `canonical_float`. A
   slice's `%v` is `"[e1 e2 e3]"`; a map's is `"map[k1:v1 k2:v2]"`,
   keys **sorted** — Go's `fmt` has sorted map keys for deterministic
   `%v` output since Go 1.12 (verified empirically), exactly this
   crate's own A2 key-sort rule, reused. The one remaining exception is
   `NaN`/`±Infinity` — out of scope for a materialized `Value` in this
   engine's own pipeline (`Stage::Canonicalize` already rejects
   non-finite floats), kept only as a defensive exception.
   `collection.go`'s own comment still calls `TopologySet`'s
   `ElementKey` *"a placeholder identity until the CIC Canonical
   Object Encoding lands"* — a property of the reference, unrelated to
   how faithfully it's ported here.
6. **Closed — the object-level WALKER, implemented in Rust
   (`engine/src/plan.rs`), closing F1's "the comparator doesn't exist
   in either language" gap (narrowed by items 4/5, closed here).**
   Ports `conformance.go`'s `FieldPlan`/`CollectionPlan`/
   `ConformancePlan`/`Evaluate`/`aggregate`/`elementKeys`/
   `resolvePath` — the piece that drives F6's per-field primitives and
   F7's element-identity primitive over a whole document. Every one of
   `conformance_test.go`'s seven end-to-end OCI vectors
   (`Conformant`/`ExtraStateFieldsAreNotDrift`/`Drift`/
   `NotComparable`/`Unobserved`/`DesiredAbsentIsConformant`, plus the
   intent/observed/plan fixtures themselves) ported case-for-case, not
   invented, plus a new test for `unknown` coverage (B2's own
   addition, no Relay equivalent) flowing through to an `Unobserved`
   verdict, matching F6's own decision for it.

   **A naming note:** this engine's `conformance.rs` (item 4) ports
   Go's `compare.go`/`observation.go`, not `conformance.go` despite
   the name — `conformance.go`'s actual content lives here, in
   `plan.rs`, named for what it is. `Observation` (introduced here) is
   new, not ported: Go's two-list envelope has no four-value
   equivalent to adapt, so this is simply a per-path lookup into B3's
   already-closed `Coverage` model, defaulting to `Unobserved` for an
   unrecorded path, mirroring Go's own default case.

   **Review-caught, fixed before merge: a genuine Relay bug, inherited
   and then corrected rather than ported.** Go's `resolvePath` splits
   a `{...}` segment on only the *first* `=` (`strings.Cut`, verified
   directly), so a multi-key identity like `"name=nic-0,zone=eu"`
   (which `Collection::element_key`, item 5, already builds and tests)
   resolves to looking for a field named `"name"` whose whole value is
   the literal string `"nic-0,zone=eu"` — matching nothing.
   `conformance_test.go` never exercises a multi-key `CollectionPlan`,
   so Relay's own tests never catch it either. Porting it here would
   have left this walker unable to resolve a path it generates from
   its own multi-key-supporting `Collection` model — fixed instead,
   per A0's standing "migration source, not a bug-for-bug contract"
   principle: a `{...}` segment is now the comma-separated `"k=v"`
   list `ElementKey` itself builds, every pair required to match.
   Proven with a new test — whose own first version was wrong in a
   way worth recording: giving intent and observed the *same* element
   value let the bug hide, since an unresolved path on both sides
   falls back to `Value::Null`, and `Null` trivially equals `Null`,
   producing a false `CONFORMANT` indistinguishable from a correct
   match. Caught by sabotage-and-restore against the pre-fix code
   (this document's own established verification discipline). The
   analogous `TopologySet` case (a bracketed segment with no `=`) has
   no working Go behavior either and is left exactly as narrow,
   named rather than silently extended.

   **What this does not do:** produce `conformance_plan_digest`/
   `observation_digest` (item 3/F5) — those commit to the *executed
   plan* and the *full observation claim*, not `SpecDigest(intent)`
   (Go's actual `IntentDigest`, also not ported: `SpecDigest` runs
   `ExpandSpec`/`normalizeNumbers` first, and this engine's
   `Normalize` stage doesn't exist yet). Nor does this decide how the
   verdict relates procedurally to the receipt (C) or to ProofTrace
   (E) — three distinct proof artifacts (F3), still not wired
   together.
7. **Closed — F5's two digests, wired to the real plan/observation
   types item 6 introduces.** `engine/src/digest_projection.rs` builds
   `PlanDigestProjection`/`ObservationDigestProjection` from
   `ConformancePlan` and a new `ObjectVerdict::consumed` field (added
   to item 6's already-merged type, since `evaluate` is the only point
   that actually knows what F5's observation projection needs — a
   value is recorded iff coverage is `Observed`, per F5's own rule),
   then digests each via section A's own `digest`/`to_canonical_json`.
   F5 already decided the preimage shape and ordering; nothing new is
   decided here.

   **Review-caught, fixed before merge: the coverage/value pairing was
   enforced by convention, not by the type.** The first version held
   `consumed` as `{coverage, value: Option<Value>}` — two independent
   fields, so `Observed` paired with `None` (or any other coverage
   paired with `Some`) was constructible and digested without
   complaint, an F5-invalid preimage producing a perfectly valid-
   looking SHA-256 hash. `evaluate` always constructs the pairing
   correctly today, but a state the contract forbids that still
   type-checks is a defect in a library whose job is proving a
   contract, independent of today's one caller. Fixed: `ConsumedField`
   is now an enum (`Observed(Value) | Absent | Unobserved | Unknown`),
   making the forbidden pairing unrepresentable — the same discipline
   this document already applies elsewhere (B3's `Option<Value>`,
   `Value::BigInt`'s invariant).

   **Tested against the exact gap F5 named, not just the shape.**
   Beyond pinning the projection's field names/ordering down as a
   concrete test (the same discipline review forced once this session
   for the shape claim itself, PR #23), one test runs `evaluate` twice
   with identical coverage but a genuinely different observed value
   and confirms `observation_digest` differs — the precise property
   F5 widened Relay's envelope-only `observationDigest()` to
   guarantee, verified as an executable fact here, not only asserted
   in prose.

   **What this does not do:** decide the conformance-verdict
   artifact's own wire layout, or wire either digest into an actual
   receipt or verdict record — no such record type exists yet, only
   the two digest values themselves.
8. **Closed — the conformance-verdict artifact's own schema text
   (`docs/VERDICT-SCHEMA.md`), AND the procedural relationship between
   the verdict, the receipt, and ProofTrace.** Four fields, none
   invented: `verdict_schema_version` (the same Versioned-property
   extension `receipt_schema_version` already used),
   `object`/`fields` (F8's `ObjectConformance`/`FieldVerdict`, string
   constants verbatim from `conformance.go`/`observation.go`), and
   `conformance_plan_digest`/`observation_digest` (items 3/7). `fields`
   needed no projection-level ordering rule, unlike `RECEIPT-
   SCHEMA.md`'s evidence arrays — it's a genuine `Map`, not a list, and
   section A's own A2 already sorts every `Map`'s keys byte-wise.

   **Closes item 2's "how the receipt relates to the verdict"
   question: independent.** Not a new design choice so much as a fact
   already true of the implementation, named rather than left
   implicit — `evaluate` (F8) takes no receipt-typed input and calls
   nothing that produces one; C2's "every materialization call
   produces a receipt" has no dependency on a verdict either. A
   receipt proves where a value came from; a verdict proves whether an
   observed value conforms to a declared intent — different questions
   over different inputs, exactly why F3 drew them as separate
   artifacts. ProofTrace stays further removed: it chains workflow
   steps, with no structural awareness of this schema's fields.

   **What this does not do:** decide whether this artifact is itself
   signed (the receipt's closed "no signature field" is specific to
   the receipt, not inherited here by default — genuinely open), or
   address a Go peer (none exists for any of F6–F10).

9. **(F11) A Go peer for F6, the first of F6–F10 to get one** — a new,
   independent Go module (`go/`, this repo's own `go.mod`, no cgo, no
   dependency on this crate or on `CIC-Relay`). `go/canonical`
   (section A's byte format) lands first as F6's own prerequisite;
   unlike the Rust side, which had to hand-port Go's behaviour because
   it had no access to Go's standard library, this package mostly just
   calls that library directly (`encoding/json` marshaling already IS
   A4's escaping, `sort.Strings` already IS A2's key order) and merges
   `CIC-Relay`'s own split two-pass logic
   (`pkg/canonicaljson`+`core/nexus/iac/{number,digest}.go`) into one
   pass, matching `canonical.rs`'s single-function contract rather than
   that migration source's internal plumbing. `go/conformance` (F6
   itself) then mirrors `conformance.rs`'s public surface exactly,
   including B3's four-value `Coverage` (not Relay's two-list
   envelope). One piece needed zero porting: the numeric comparator's
   string grammar, which `rat_from_string` spent two PR #25 review
   rounds hand-porting on the Rust side specifically because Rust has
   no access to `math/big.Rat.SetString` — this package just calls that
   function directly, exactly as `compare.go` already does.

   **What this does not do:** port F7/F8/F9 to Go, or wire up a
   cross-language differential corpus — each side is independently
   verified against the decided contract here, not against each other
   yet (that is a separate, later step).

10. **(F12) A Go peer for F7, one step further** — `go/collection`,
    same pattern as F11. Needed even less hand-porting: `collection.rs`'s
    own `go_display`/`go_float_display` exist only because Rust has no
    built-in equivalent of `fmt.Sprintf("%v", ...)`; this package calls
    it directly. The only hand-written piece is a recursive
    non-finite-value pre-check (`%v` would print `"NaN"`/`"+Inf"` as if
    it were a stable identity). A missing map-topology key field needed
    no special case either — Go's own `m[k]` zero-value-on-missing-key
    plus `fmt.Sprintf("%v", nil)` already produce `"<nil>"` for free,
    where `collection.rs` had to hand-replicate it.

    **Found a real, previously-uncaught cross-language divergence
    while building this — the first time the Go-peer work itself found
    a bug, rather than a human review:** `collection.rs`'s own
    `go_float_display` folded `-0.0` to `"0"`, assuming (without
    checking) that `%v` treats zero the way section A's `canonical_float`
    does. Real Go's `%v` on a genuine negative zero prints `"-0"` —
    verified via `json.Unmarshal([]byte("-0.0"), &f)` then
    `fmt.Sprintf("%v", f)`. `go/collection`'s own test for the same
    vector caught it immediately, since it has nowhere for the bug to
    hide; fixed on the Rust side in the same PR.

    **Applying PR #31's review lesson proactively:**
    `CollectionTopology.Valid()` is checked at `ElementKey`'s own
    boundary from the start, rather than waiting for a review round to
    ask for it a third time.

    **What this does not do:** port F8/F9 to Go, or the cross-language
    differential corpus (same two items item 9 already named).

**Also established: three distinct proof-adjacent artifacts, not one.**
Building on E1's ProofTrace-vs-receipt distinction: (1) ProofTrace's
chain-of-custody (which steps ran, with which I/O hashes), (2) the
materialization receipt (section C — digests plus provenance-derived
default/derivation evidence, per what C3/C5 actually decided; coverage
does not additionally need to become part of this artifact for custody
purposes, per B7 — a different, smaller "audit convenience" question
remains open per this section's own item above), and (3) the
conformance/drift verdict (`conformance.go` — whether an observed value
matches a declared intent) are three separate facts. `C3` already
flagged `conformance_plan_digest`/`observation_digest` as "section F's
territory, not provenance" for exactly this reason — the verdict is not
part of the receipt, whatever the receipt eventually turns out to
include.

---

### G. Differential conformance

**Status:** **PARTIALLY DECIDED, not closed** — and cannot fully close
until F's comparator implementation lands (B2, C5, C3's
unresolved/unknown-markers row, and C3's `conformance_plan_digest`/
`observation_digest` rows have all since closed and no longer block
this). The comparison *harness structure* is fixed; full test
*coverage* is not yet possible.
**Blocks:** nothing further — this is the last section
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#g--differential-conformance-partially-decided-not-closed`

```text
same authored input
+ same schema/primitives version
=
same canonical materialized bytes
+ same receipt semantics
```

Not "the same object, roughly" — byte-identical materialized output and
semantically-identical receipts, or the pair does not actually prove
anything about parity.

1. **Closed:** `conformance/` already holds a language-independent
   vector corpus with a harness that enforces it can't trivially pass
   (`engine/tests/conformance.rs`, verified directly — empty corpus
   fails, a group with no accepted vector fails). This effort adds a new
   group to it (e.g. `materialization/`), not a second, parallel
   mechanism. **Precision, not assumed:** that harness file has a
   *generic* part (walks every group, checks only corpus invariants)
   and a *separate*, named, hardcoded `reader_vectors()` test that
   actually runs the engine and checks outcomes — adding a
   `materialization/` group gets the generic checks for free, but an
   analogous hand-written `materialization_vectors()` test, actually
   exercising the resolver, still needs writing.
2. **Closed, recovered from A0/F, not newly decided:** `check_grammar.py`
   validates static schema structure (field definitions); this effort
   resolves value instances at runtime. Different axes, nothing to
   differentially agree or disagree about — it does not join as a third
   oracle. Stated explicitly so the split reads as a decision.
3. **Closed — the harness structure, per D1.4's own forward note that
   "compare the receipts" can't be one byte-equality check:** three
   separate questions, checked in order —
   ```text
   1. Comparability gate (D2): grammar digest + primitive release +
      schema identity/version/digest + authored input digest MUST
      match; validator/engine identity MUST differ and is excluded.
      Mismatch -> NOT_COMPARABLE (not a divergence). Stop.
   2. Value equality (A): canonical materialized bytes, compared
      directly. Mismatch -> DIVERGENCE.
   3. Receipt semantic equality (C, D1.4): construct each receipt's
      comparison PROJECTION (every field whose semantics must agree,
      excluding validator/engine identity), canonicalize each
      projection (A's format, reused), compare bytes. Mismatch ->
      DIVERGENCE.
   ```
   **Correction (review-caught):** step 3 originally said "remove the
   engine-identity field" — a wire-level operation naming a field path
   that doesn't exist yet, since C4 (the receipt's exact schema/layout)
   is still open. Reframed as a semantic *projection* so this doesn't
   quietly decide C4's layout from inside G; once C4 fixes the real
   field layout, the projection's mechanical definition follows, G does
   not need revisiting.

   This harness gets its **own** verdict vocabulary
   (`NOT_COMPARABLE`/`DIVERGENCE`) — deliberately distinct from
   `conformance.go`'s `CONFORMANT`/`DRIFT`/etc. (F3, artifact 3), which
   answers whether an *observed* value matches a *declared intent*, a
   different question than whether *two implementations* agree. Sharing
   the word `NOT_COMPARABLE` is a coincidence of English, named before
   it becomes the third or fourth conflation this effort has had to
   correct (after ProofTrace/receipt and receipt/verdict).

**Not closed, and cannot be yet:** full test coverage. Any vector
exercising the comparator/verdict artifact — including
`conformance_plan_digest`/`observation_digest` — is out of scope for
the harness today, not because their semantics are undecided (F5
settled those) but because F's comparator has no implementation in
either language yet, so there is nothing to vector against — named as
a gap, not silently passed or skipped. C5's evidence record and C3's
`unresolved/unknown markers` row have since closed (the latter via
B7: coverage is part of what step 2's canonical-byte comparison already
catches, with no separate receipt field needed) and no longer belong
on this list.

**This closes the first pass through A–G.** Every section has at least
a decided core; B, C, E and F remain explicitly partial, each with
named, specific open items. The next work is closing those — not
starting new sections.

## Addendum: `PRIMITIVE-IR.md`'s open question #2

`PRIMITIVE-IR.md`'s own five open questions seeded part of this effort
(#1 → C1, #3 → A, #4 → D). Of the remaining two, **#2 is now answered**
— by reconciling a real internal tension in `BOUNDARY.md`, not a
clean, uninterpreted recovery (review caught an overclaim here, see
below) — and **#5** ("which parts are stable API and which are
engine-internal") stays genuinely open; nothing grounds an answer to
it yet, and this file does not invent one.

**#2** asks whether the IR carries unresolved references explicitly,
or whether resolution is total. `BOUNDARY.md`'s Constructors text
states, as one of `Validated<Materialized<PrimitiveDocument>>`'s
guarantees: *"every reference is resolved or explicitly unresolved."*
**Correction (review-caught):** citing only that sentence and calling
the question "recovered" overstated it — `BOUNDARY.md` also forbids,
in the same document, *"an object with references still unresolved"*,
and separately requires materialized validation to leave *"references
resolvable."* Taken at face value, those three sentences contradict
each other. The actual decision here is reconciling them: *"still
unresolved"* (forbidden) describes resolution as an **incomplete,
in-progress** state, the same register as its list-mates ("a partially
interpreted document," "defaults still unapplied"); *"explicitly
unresolved"* (permitted) is resolution having **run to completion**
with a terminal, determinate answer — no binding found, not "haven't
looked yet." `"resolvable"` is read here as *"reached a terminal
result"*, not *"the target must exist"* — favoring the constructor's
more specific, explicit wording over the validation bullet's looser
restatement, named explicitly as this document's own interpretive
reading of a genuine ambiguity, not a fact `BOUNDARY.md` states
unambiguously. With that distinction drawn: an explicitly-unresolved
reference, reached as resolution's own terminal output, is a
legitimate, representable materialized state, not a failure mode.
**Named so it isn't conflated with a different gap:** `cic-primitives`'
own D-014 left reference *target-existence* checking unbuilt (no type
registry to confirm a declared target Kind is real) — a static,
schema-level question, explicitly scoped by D-014 itself as "a
separate item," not this one. This question is instance-level: does
*this* authored reference, in *this* composition, resolve within that
composition's own graph? Recovered here deliberately **outside**
section B, not folded into it — B1's three-axis count is already
closed as final, and treating this as a fourth value on an existing
axis (or a fourth axis) would be a new design decision this recovery
does not make. **Exactly how** "explicitly unresolved" gets
represented stays open.
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#addendum-primitive-irmds-open-question-2`

---

## Related

- `README.md` — the engine's contract and the archived-model lesson this
  file exists to avoid repeating
- `docs/PRIMITIVE-IR.md` — the IR's required properties and its own five
  open questions; #1/#3/#4 are answered (C1/A/D), #2 is answered (see
  addendum above), #5 remains genuinely open
- `docs/BOUNDARY.md` — the custody boundary, the five forbidden states, the
  defaultability-by-Role-axis table, and the receipt sketch this file builds
  decision tracking on top of
- `dependency.yaml` — the still-unpinned `cic-primitives` dependency; closing
  it is a precondition for section D, not an independent task
