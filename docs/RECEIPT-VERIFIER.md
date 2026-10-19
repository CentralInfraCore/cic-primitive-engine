# The receipt verifier

`engine/src/receipt.rs` (Rust) and `go/receipt` (Go) check a materialization
receipt whose layout `RECEIPT-SCHEMA.md` decided. They are verifiers. **No
receipt producer exists in this crate**, and none is added by this change:
materialization, and the evidence a default or a derivation leaves while it
runs, belong to the environment (`BOUNDARY.md`). The receipt contract, the
digest format, D-018 and the scope boundary are not changed here.

## What a success establishes

Structural and content consistency, and nothing more.

- All 15 required top-level fields are present, correctly typed, in a closed
  layout (no other member), under the sole wrapper key `materialization`.
- `receipt_schema_version` is the integer `1`. A float (`1.0`) is not an
  integer.
- Every digest is `sha256:` followed by 64 lowercase hex characters. A base64
  or bare-hex spelling is refused: re-spelling the same SHA-256 bytes is the
  producer's job, and only an equal-bytes re-spelling is a valid conversion; a
  different hash payload or a different canonicalization does not become
  equivalent by changing the spelling.
- `applied_defaults` and `derived_values` are strictly ascending by `.path`
  (raw UTF-8 byte order; a repeated path is a rejection, not a tolerated
  duplicate), every `rule`, `path` and `value_digest` has its decided form
  (`applied_defaults[].rule` is the literal `schema-default`; a derived rule is
  `name-vN`; paths are `$.`-rooted with non-empty segments), and each
  `derived_values[].inputs` is a strictly ascending, deduplicated path set.
- The evidence lists and every `inputs` list are required arrays: `[]` when
  empty, never null. In Go a `nil` slice is the null spelling, so the typed API
  refuses it (`R-RECEIPT-SHAPE`) and `Parse` turns a JSON `[]` into a non-nil
  empty slice. Rust's `Vec` cannot be null, so the invariant already holds.
- `output_digest` equals `materialized_object_digest` of the object it was
  handed.
- `input_digest` equals the A7 digest of the canonical form of the authored
  input it was handed. The input is a processed value, not raw source bytes,
  and no YAML reader is assumed or used.
- The object's defaulted fields (`schema_default` provenance) and derived
  fields (`derived` provenance, intent- or state-side) are exactly the ones the
  receipt lists, each entry's `value_digest` equals the digest of that field's
  value, and no object field is listed twice.

## What it does not establish

- That a default was applied or a derivation ran. A receipt that passes is
  consistent with the object; it is not evidence about the execution that
  produced the object. `rule` and `inputs` are the producer's statements.
- That `inputs` is what the derivation actually read. The finished object cannot
  show that, so no input path is required to exist in the object, and none is
  looked up. Form and order are all that is checked.
- Anything about the identifiers. `grammar_*`, `primitive_*`, `schema_*` and
  `engine_*` are claims: their form is checked, the release signature and the
  origin of the pinned schema are not authenticated, and no signature is
  verified (the schema has none). `engine_identity` names the implementation
  that materialized the data, not this verifier.

## Evidence needs an explicit path binding

A `MaterializedObject` is keyed by flat field names; a receipt names fields by
path. The mapping between them is not specified anywhere in the contract, so
this verifier does not guess it. The caller passes a `PathBinding` (receipt
path to object key). An evidence entry whose path is not in the binding is
refused with `E_RECEIPT_UNBOUND`; nothing is accepted partially.

With both evidence lists empty and no defaulted or derived field in the
object, no binding is needed, and that case verifies completely.

## Rejections

| code | meaning | rules |
|---|---|---|
| `E_INVALID_RECEIPT` | not a well-formed receipt | `R-RECEIPT-SHAPE`, `-VERSION`, `-DIGEST-FORMAT`, `-EVIDENCE-FORM`, `-ORDER` |
| `E_RECEIPT_MISMATCH` | well formed, but disagrees with the artifacts | `R-RECEIPT-BINDS-INPUT`, `-BINDS-OUTPUT`, `-EVIDENCE-MEMBERSHIP`, `-EVIDENCE-VALUE` |
| `E_RECEIPT_UNBOUND` | an evidence path has no explicit binding | `R-RECEIPT-PATH-BINDING` |

The reported location is a path inside the receipt
(`$.materialization.applied_defaults[0].value_digest`) or, for an object field
the receipt does not list, `object/<key>`. Checks run in a fixed order (shape
and structure in declared field order, `input_digest`, `output_digest`,
evidence entries in list order, the object's own fields in key order) so both
languages name the same location.

## Corpus

`conformance/differential/receipt/` holds the shared vectors; see that
directory's README for the format. The rejection vectors cover a changed input
digest, a changed output digest, a missing field, an unsupported version, a
version that is not an integer, unsorted and duplicated evidence, unsorted and
duplicated `inputs`, missing path binding, and evidence that disagrees with
the bound object field.
