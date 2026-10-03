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

**A0 and A are now closed; B and C are PARTIALLY DECIDED, not closed**
(2026-10-03) — see `docs/A0-INVENTORY.md` and
`docs/MATERIALIZATION-SPEC.md`. B's `missing`/`unknown` question (B2)
remains open and could still change B1's three-axis count — review
caught that an earlier version of this file claimed B was fully closed
while its own B2 was marked OPEN, a real contradiction, not an
admin detail. C built on B's closed parts (B1/B3/B4/B5) without waiting
for B2, and is itself now partially decided: C1/C2 are settled, C5's
classification rule is settled but its entry-evidence payload is open,
and C3 (field list)/C4 (schema home) each have a part that's really
section D's, F's, or B2's job, reserved rather than guessed at. **D is
next**, and also unblocks the version-identity fields C3 reserved for it.

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

**Status:** **PARTIALLY DECIDED, not closed.** B1 (the axis model, modulo
below)/B3 (type shape)/B4 (A0.4 conformance resolution)/B5
(two-trigger `default_injection`) are settled. B2 (`missing`, `unknown`)
is **OPEN** and blocks full closure — review caught that an earlier
version of this status line said "DECIDED" while the spec's own B2
said OPEN, a real contradiction. Resolving `missing` may even add a
**fourth axis** (field-value presence/existence), which would revise
B1, not just fill in a blank — so B1 itself is not 100% final either
until B2 closes.
**Blocks:** the materialized output's type shape (C, E); `default_injection`
correctness; full closure of this section
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#b--semantic-state-model-partially-decided-not-closed`

`docs/BOUNDARY.md`'s "five distinct statements" are not five values of
one enum — they're points on **three separate, orthogonal axes**:
**capability** (D-012's `implemented`/`not_implemented`/`deprecated`,
static per device binding), **coverage** (`observed`/`absent`/
`unobserved`, dynamic per observe call, already landed in Go as
`CoverageState`), and **provenance** (`authored`/`schema_default`/
`derived`, intent-side only).

Two terms flagged as genuinely open, not silently settled — review
caught that an earlier draft wrongly resolved the first one:
- `missing` stays **OPEN**. An earlier draft said it's the same concept
  as coverage's `absent`; review correctly caught that `BOUNDARY.md`
  calls all five of its terms "different statements" (not four plus a
  synonym), and `observation.go`'s actual `absent` (envelope
  affirmatively says not-there) vs. `unobserved` (envelope says nothing)
  split has no obvious slot for it either. Neither repo defines it
  distinctly from the other four — left open on the same footing as
  `unknown`, not decided.
- `unknown` is **ungrounded in both repos** — `BOUNDARY.md` names it and
  never defines it, and nothing in `cic-primitives`' decision log or
  `core/nexus/iac` gives it a concrete shape either. The spec offers a
  best-reasoned candidate (a fourth coverage value, for "the device
  reported an indeterminate value," distinct from an affirmed absence)
  explicitly as a new proposal for review, not a recovered fact.

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
F for `conformance_plan_digest`/`observation_digest`, B2 for
unresolved/unknown markers), reserved here rather than guessed at.
**Blocks:** differential conformance (G); proof-chain integration; full
closure pending D, F, B2, and C5's evidence-record specification
**Decision ref:** `docs/MATERIALIZATION-SPEC.md#c--receipt-schema-partially-decided-not-closed--same-posture-as-b`

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
   `unresolved/unknown markers` cannot be specified until B2
   (`missing`/`unknown`) has a real shape. `conformance_plan_digest` is
   section F's territory with no landed precedent either;
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

**Status:** OPEN
**Blocks:** differential conformance (G); the same false-provenance failure
mode this engine's own still-unpinned `dependency.yaml` already names
**Decision ref:** —

1. Does the materialized output reference the `cic-primitives` grammar
   version as one number or two (engine version and schema version kept
   separate)? (`PRIMITIVE-IR.md` open question #4)
2. Do the Go and Rust libraries release in lockstep (one version number
   covers both), or independently — and if independently, how does each
   declare which grammar version it implements, so a mismatched pair is
   detectable rather than silently producing "same-looking" but
   differently-sourced output?

---

### E. Boundary enforcement

**Status:** OPEN
**Blocks:** F, G — nothing downstream is meaningful if a module can bypass
materialization
**Decision ref:** —

```text
Raw<T>
  ↓
Materialized<T>
  ↓
Validated<T>
  ↓
Module
```

1. Where does enforcement actually live — the host (Relay) layer refusing to
   hand a module anything but validated output, or is every guest module
   independently responsible for calling the resolver itself?
2. `docs/BOUNDARY.md` already records that Rust's `Materialized<T>`/
   `Validated<T>` private-constructor guarantee does not survive a
   process/WASM boundary, and is "only approximately" achievable in Go even
   in-process. Given that, what — concretely — stops a Go-authored WASM guest
   from skipping the resolver, if not the type system? A host-side gate that
   refuses unresolved input before it reaches the guest's entry point is one
   candidate; name the actual mechanism, don't leave it implicit.
3. Is the enforcement point identical for a Go guest and a Rust guest, or
   does the language difference require two different mechanisms?

---

### F. Output symmetry

**Status:** OPEN
**Blocks:** proof-chain completeness — today only the input side is in scope
**Decision ref:** —

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

1. `docs/BOUNDARY.md`'s "output is a new claim" principle requires the same
   normalize/validate/canonicalize treatment on a module's *output*
   (observation), not just its input. Does the planned Go/Rust resolver pair
   cover this symmetric path, or is input resolution the only scope for now?
2. CIC-Relay's `core/nexus/iac/conformance.go` already hand-builds part of
   this (intent vs. observed comparison, `UNOBSERVED`/`CONFORMANT`/`DRIFT`
   verdicts) for one vertical slice (OCI), independently of this effort. Does
   the new resolver eventually replace/drive that code, or do the two stay
   separate indefinitely? If separate, that is itself a second place the same
   judgment could silently diverge — worth stating as a deliberate, scoped
   decision rather than an oversight (cf. D-017's explicit scoping of
   `deprecated` in cic-primitives).

---

### G. Differential conformance

**Status:** OPEN
**Blocks:** nothing — this is the last section, and the point of the whole
exercise
**Decision ref:** —

Only meaningful once A–F are closed:

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

1. Does `cic-primitive-engine/conformance/` (the existing language-independent
   `input.yaml`/`expected.yaml` corpus, today covering only the `reader`
   group) get extended to carry Go/Rust materialization vectors, or is a
   separate corpus built for this purpose?
2. Does `cic-primitives`' own Python grammar checker (`check_grammar.py`)
   join as a third differential oracle, or does it stay scoped to static
   schema-structure validation only (it validates field *definitions* —
   e.g. is `role: config` legal — not runtime value-instance resolution,
   which is this effort's actual target; confirm this stays a deliberate
   split, not a gap)?

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
