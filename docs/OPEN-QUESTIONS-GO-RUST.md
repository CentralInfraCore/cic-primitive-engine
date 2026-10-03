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
settled, C5's classification rule is settled but its entry-evidence
payload is open, and C3 (field list)/C4 (schema home) each have a part
that's really section D's or F's job, reserved rather than guessed at.
C3's `unresolved/unknown markers` row was specifically reserved pending
B2 — B2 closing now clears that blocker, but the row's actual shape is
still not decided; closing it is separate follow-up work for C, not
done by B2's closure.

**D is now closed too** — four identifiers, not one or two (grammar
digest, primitive release identity, schema version, validator/engine
identity), filling the version-identity row C3 reserved for it. Go and
Rust release independently; the full semantic-input identity (grammar
digest + primitive release + schema version + input digest, everything
except validator/engine identity, which is supposed to differ) is the
comparability gate section G's differential conformance checks before
comparing output. **E is now PARTIALLY DECIDED** — host-side enforcement
at a specific, real chokepoint already found in `CIC-Relay/core/cabinet/
service.go` (E1, E2a, E3 closed), with the type's exact representation,
the receipt-transport wire shape, and a real, found gap (the dispatch
loop's `authContextJson` isn't a genuine actor identity yet) left open
(E2b and beyond) — the identity gap is a standing dependency for anyone
implementing E, not just a documentation loose end.

**F is now PARTIALLY DECIDED too** — the model already covers output
symmetry by construction (no new data shape needed: B3's coverage/
provenance axes, already orthogonal not input-vs-output split, and
section A's canonical form already serve both; section C's every-call
receipt *production* is direction-agnostic, though whether coverage
itself ever gets projected into the receipt's fields is separately
still open), and `core/nexus/iac`'s comparator
(`compare.go`/`observation.go`/`conformance.go`) is confirmed migration
source under A0's existing meta-decision, not a permanent fork — made
explicit for this axis rather than left looking undecided. Open: the
comparator has no implementation in the new lib yet, in either language,
whether/how `MaterializedField.coverage` projects into the receipt, and
how the materialization receipt relates to the separate conformance/
drift verdict (a third, distinct proof artifact alongside ProofTrace and
the receipt) isn't decided.

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
vector touching C3's still-open fields, C5's evidence record, or F's
unbuilt comparator is out of scope until those close (B2 no longer
belongs on this list — it closed in a later pass). Every section A–G
now has at least a decided core — B is fully closed; C, E and F remain
explicitly partial. The next work is closing those named items, not
opening new sections.

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
itself, as a known placeholder). The Rust side does not yet *implement*
most of this — that's follow-up work (roadmap step 4), tracked but not
part of closing this section.
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
verification detail, and exactly what remains open (Unicode normalization,
`TopologySet` order, and the fact that the Rust peer for everything except
number canonicalization still has to be written).

---

### B. Semantic state model

**Status:** **CLOSED.** B1 (axis model)/B2 (`missing`/`unknown`
resolved)/B3 (type shape)/B4 (A0.4 conformance resolution)/B5
(two-trigger `default_injection`) are all settled.
**Blocks:** nothing further — unblocks full closure of C, E
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

---

### C. Receipt schema

**Status:** **PARTIALLY DECIDED, not closed** — same posture as B. C1
(sibling artifact, not IR-embedded) and C2 (produced every
materialization call) are settled. C5's *classification* rule is settled
(`provenance` alone decides whether an `applied_defaults`/
`derived_values` entry exists — no second source needed for that
yes/no), but C5's entry *evidence* (`rule`/`inputs`/`value_digest`) is
open: the three-value `provenance` enum cannot supply it, and the
execution record it would come from isn't specified yet. C3 (full v1
field list) and C4 (schema home) are only partially decided — each has a
part that's really another section's job (D for version-identity fields,
F for `conformance_plan_digest`/`observation_digest`). The
`unresolved/unknown markers` row was reserved pending B2 — B2 has since
closed (`missing` ≡ coverage's `absent`; `unknown` is a new, fourth
coverage value), clearing that specific blocker, but the row's actual
receipt-field shape is still not decided; that's separate follow-up
work for C, not something B2's closure did on its own.
**Blocks:** differential conformance (G); proof-chain integration; full
closure pending D, F, and C5's evidence-record specification
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#c--receipt-schema-partially-decided-not-closed`

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
3. **Partially decided, re-grounded after review:** the minimum field set
   reconciles `BOUNDARY.md`'s sketch with what's actually proven. Only
   `BOUNDARY.md`'s own fields are decided: the two digests and
   `applied_defaults`/`derived_values` (derived from B3, see item 5
   below). **`signature` is now OPEN, not decided** — an earlier version
   claimed "Relay already signs this," which is false (no landed
   precedent exists; the sketched `proof` index that would carry it is
   itself deferred). `grammar_version`/`primitive_release`/
   `schema_version`/validator identity are reserved for section D.
   `unresolved/unknown markers` was reserved pending B2
   (`missing`/`unknown`); B2 has since closed with a real shape for
   both terms (`missing` ≡ coverage's `absent`, `unknown` a new fourth
   coverage value), clearing that blocker — but this row's actual
   receipt-field shape is still **not decided here**, only unblocked.
   `conformance_plan_digest` is section F's territory with no landed
   precedent either;
   `observation_digest` **is** landed — but as a field of
   `ConformanceResult` (the drift verdict), a different artifact from
   this receipt, not simply reusable here.
4. **Partially decided:** the schema does **not** live in
   `cic-primitives`' `schemas/atomic/`/`schemas/aggregate/` — giving the
   receipt a primitive's schema home would reintroduce the exact
   category error `BOUNDARY.md`'s regress argument (item 1) exists to
   avoid. Whether it lives in this repo instead, or needs its own
   location, is deferred until item 3's field list is actually complete.
5. **Classification closed; evidence open — review caught an
   overclaim here too.** B3's `provenance` was first wrongly scoped
   "intent/input side only," which couldn't produce `BOUNDARY.md`'s own
   state-side `derived_values` example (`$.state.effective_state`,
   derived from `$.state.admin_state`/`$.state.oper_state`). Fixed:
   `authored`/`schema_default` stay intent-side only (the latter also
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
   while the entry's **evidence payload** comes from a separate
   materialization/derivation execution record this section does not
   yet specify. The receipt is a deterministic projection of both, not
   of `provenance` alone.

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
settled. E2b (exact type shape, receipt-transport wire shape), whether
native and WASM modules get the *same* trust/policy treatment inside
the mandatory boundary (a different question from E1 — review caught an
earlier draft conflating the two), how a real actor identity gets
threaded through (a genuine gap found in the live code, not
hypothesized), and the guest-side digest-check shape are open.
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
   **A real, found gap, also open:** `authContextJson := step.ComponentID`
   is the calling component's own ID, not an actual actor identity
   `acl.go`'s `ACL.Allows` could evaluate — so B5's capability/ACL gates
   can't run meaningfully at this point *yet*. This directly answers
   what B5 left open (what prevents a module from reading a value it
   isn't entitled to): this same gate, once a real identity is threaded
   through — not a separate mechanism, and not resolved by this section.
   **Also open, distinct from item 1's structural claim:** whether
   `NativeImpl` (first-party, non-WASM) modules get the *same*
   capability/ACL treatment as `WasmCode` modules once past the
   mandatory chokepoint, or a different trust tier there. "Same
   chokepoint for both" (item 1, closed) is not the same claim as "same
   policy for both" (open).
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
fixed away from) and F2 (the comparator is migration source under A0's
existing meta-decision, made explicit for this axis) are settled.
**Open:** whether/how `MaterializedField.coverage` gets projected into
the receipt (C3 carries no coverage field today — a second overclaim
review caught in the same paragraph), the comparator/verdict logic's
actual implementation (in either language), and how the materialization
receipt relates to the separate conformance/drift verdict artifact.
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
   there today, and `conformance_plan_digest`/`observation_digest`
   remain exactly as open as C3 already said. What doesn't exist yet, in
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

**Also established: three distinct proof-adjacent artifacts, not one.**
Building on E1's ProofTrace-vs-receipt distinction: (1) ProofTrace's
chain-of-custody (which steps ran, with which I/O hashes), (2) the
materialization receipt (section C — digests plus provenance-derived
default/derivation evidence, per what C3/C5 actually decided; **not**
restated here as already carrying coverage/observation evidence, which
stays open per this section's own item above), and (3) the
conformance/drift verdict (`conformance.go` — whether an observed value
matches a declared intent) are three separate facts. `C3` already
flagged `conformance_plan_digest`/`observation_digest` as "section F's
territory, not provenance" for exactly this reason — the verdict is not
part of the receipt, whatever the receipt eventually turns out to
include.

---

### G. Differential conformance

**Status:** **PARTIALLY DECIDED, not closed** — and cannot fully close
until C3's open fields/evidence record and F's comparator implementation
do (B2 has since closed and no longer blocks this). The comparison
*harness structure* is fixed; full test *coverage* is not yet possible.
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
touching C3's still-open fields (its `unresolved/unknown markers` row
specifically — B2 itself has since closed, but that row's receipt-field
shape has not), C5's evidence record, or F's not-yet-implemented
comparator is out of scope for the harness today — named as a gap, not
silently passed or skipped.

**This closes the first pass through A–G.** Every section has at least
a decided core; B, C, E and F remain explicitly partial, each with
named, specific open items. The next work is closing those — not
starting new sections.

---

## Related

- `README.md` — the engine's contract and the archived-model lesson this
  file exists to avoid repeating
- `docs/PRIMITIVE-IR.md` — the IR's required properties and its own five
  still-open questions, several of which this file restates with Go/Rust
  context attached
- `docs/BOUNDARY.md` — the custody boundary, the five forbidden states, the
  defaultability-by-Role-axis table, and the receipt sketch this file builds
  decision tracking on top of
- `dependency.yaml` — the still-unpinned `cic-primitives` dependency; closing
  it is a precondition for section D, not an independent task
