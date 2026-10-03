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
Sections A–G below are this pipeline's open decisions, in the order they
must be closed — each later section assumes the ones before it are settled.

## Decision order: A → B → C → D → E → F → G

Section A must close first: without a canonical byte representation, it is
not possible to even state objectively whether the Go and Rust
implementations produced "the same" output.

---

### A. Canonical representation

**Status:** OPEN
**Blocks:** everything below — B through G all assume a canonical form exists
**Decision ref:** —

1. What is the canonical byte representation of the materialized long-form
   output — a YAML profile, canonical JSON, or a format this engine defines
   outright? (`PRIMITIVE-IR.md` open question #3, never closed)
2. Within that choice, pin down explicitly:
   - key ordering
   - number/string/bool/null representation (e.g. `16` vs `16.0`, the exact
     case this engine's `Value` enum already separates `Int`/`Float` for)
   - Unicode normalization
   - list/sequence ordering
   - map canonicalization
   - the exact bytes that go into the digest (is it the canonical form
     itself, or a transform of it?)
3. Is there an existing convention to reuse — e.g. does CIC-Relay's
   `cic-canonical` crate already define this, or was it built for a
   different purpose (canonical JSON for the Vault/trust-flow FFI boundary,
   not for primitive materialization)? Needs checking before assuming reuse.
4. Byte-equality via a shared conformance corpus, or does each language also
   need its own written canonical-writer spec independent of test vectors
   (so a vector gap doesn't silently hide a canonicalization bug)?

---

### B. Semantic state model

**Status:** OPEN
**Blocks:** the materialized output's type shape (C, E); `default_injection`
correctness
**Decision ref:** —

`docs/BOUNDARY.md` names five distinct statements that must never collapse
into one: `missing`, `unknown`, `not_observed`, `not_implemented`, and a
schema-applied default. For each:

1. Which of these is a **data value**, which is **metastate**, which is
   **provenance**, and which is a **capability claim** (e.g. `not_implemented`
   is D-012's hard-reject capability statement, not a data value at all)?
2. How does this surface in the materialized output's type — a tagged union
   per field, a side-channel status map, or something else?
3. How does `access.default_injection` (what a requester *without* access
   sees) stay distinct from the field's real internal state in the same
   output — two separate fields, or a view-dependent filter applied only at
   serialization/response time (never present in the internally-held
   materialized form at all)?

---

### C. Receipt schema

**Status:** OPEN
**Blocks:** differential conformance (G); proof-chain integration
**Decision ref:** —

1. Is the receipt part of the IR document, or a sibling artifact bound by
   digest? (`PRIMITIVE-IR.md` open question #1)
2. Is a receipt produced on every materialization call, or only at specific
   points (e.g. release-time, not every runtime resolution)?
3. Minimum candidate field set (per `BOUNDARY.md`'s sketch) — decide which of
   these are actually in v1, not just possible:
   ```text
   grammar_version
   primitive_release
   schema_version
   input_digest
   materialized_digest
   field provenance (authored / defaulted / derived / observed / unavailable)
   defaults applied
   derivations applied
   unresolved/unknown markers
   validator/version identity
   ```
4. Where does the receipt's own schema live — does it get a home in
   `cic-primitives`/`cic-schema-registry` as a real, versioned schema, so
   both the Go and Rust implementations are provably targeting the same
   definition rather than each hand-rolling a struct that happens to agree
   today?

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
