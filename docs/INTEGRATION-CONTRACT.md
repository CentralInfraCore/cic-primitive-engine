# Integration contract: what the environment supplies, what the engine returns

Status vocabulary (per statement): **implemented** = in code on `main`, covered by tests; **scaffold** = in code, deliberately not
wired or exercised end to end; **concept** = written down, no runtime counterpart. A statement without a status is a description of
a document, not a claim.

This document is the integration contract of the existing `materialized` + `conformance` + `verdict` functionality, proven on one
slice: an OCI VCN (`fixtures/vcn-v1`, from cic-factory PR #37, nine cases). **The engine builds nothing new for it.** Everything
before the engine (reading the OCI answer, normalizing it, mapping OCI names to registry names, deciding what the coverage of each
path is, resolving references, applying defaults, validating) is the environment's job (`BOUNDARY.md`, `MATERIALIZATION-SPEC.md`,
"Scope correction"). *implemented* as a boundary.

## What the environment must supply

For `evaluate_verdict(intent, observed, observation, plan)` (Rust) / `verdict.EvaluateAndBuild` (Go):

1. **`intent: MaterializedObject`** and **`observed: MaterializedObject`**, each built with `MaterializedObject::try_new(fields,
   &expected_keys)` / `NewMaterializedObject`. The key set of the fields must be **exactly** `expected_keys`; any missing or extra
   key is `E_INCOMPLETE_OBJECT`. *implemented.* The environment decides `expected_keys` (the schema's keys); the engine only
   checks the set. *implemented.*
2. **A per-field `capability`** (`implemented` | `not_implemented` | `deprecated`) and **evidence**: on the intent side
   `Authored(Some(v))`, `Authored(None)` (declared absent), `SchemaDefault(v)`, `Derived(v)`; on the observed side `Observation(c)` /
   `DerivedObservation(c)` with `c` one of `Observed(v)`, `Absent`, `Unobserved`, `Unknown`. *implemented.*
3. **An `Observation`**: the coverage (`observed` | `absent` | `unobserved` | `unknown`) of **every path the plan names**. A path
   not listed is `unobserved`. *implemented.* Deciding which of the four a given OCI answer is (a 404, a timeout, a missing source
   of a derived value, a malformed field) is the environment's; the rules are in `fixtures/vcn-v1/README.md`. *concept* on the
   engine side (it takes the decision as input), *implemented* only as fixture files.
4. **A `ConformancePlan`**: scalars `{path, compare: exact|numeric}` and collections `{path, topology: map|set|atomic, keys,
   elements}`. *implemented.* Which paths are planned is the environment's decision; see "The open question".

## What the engine returns

`ConformanceVerdict { verdict_schema_version: 1, object, fields, conformance_plan_digest, observation_digest }` and its canonical
bytes (`verdict_canonical_bytes` / `Verdict.CanonicalBytes`). *implemented*, byte-identical in Rust and Go on every vector of
`conformance/differential/verdict/`.

* `fields`: exactly one entry per path the plan classified, with `CONFORMANT` | `DRIFT` | `OBSERVED_ABSENT` | `UNOBSERVED` |
  `NOT_COMPARABLE`.
* `object`: `DRIFT` if any field is `DRIFT` or `NOT_COMPARABLE`, else `INCOMPLETE` if any field is `UNOBSERVED`, else `CONFORMANT`.
* `evaluate_verdict` binds the verdict to the plan it ran; `build_verdict` is the low-level builder and cannot check that (use the
  former). *implemented.*

## How absent, unobserved and unknown are treated

| Coverage | Intent declares a value | Intent declares nothing |
|---|---|---|
| `observed` | `CONFORMANT` if equal, `DRIFT` if not, `NOT_COMPARABLE` if the kinds differ | `NOT_COMPARABLE` if a value was observed (see below) |
| `absent` (authoritative) | `DRIFT` | `OBSERVED_ABSENT` |
| `unobserved` | `UNOBSERVED` | `UNOBSERVED` |
| `unknown` | `UNOBSERVED` (the same verdict as `unobserved`; the digests differ) | `UNOBSERVED` |

`unobserved` and `unknown` can never produce `CONFORMANT`: the vectors `vcn_not_found` (a 404 `NotAuthorizedOrNotFound`, every path
`unknown`), `vcn_transport_error` (every path `unobserved`), `vcn_partial_read` (a derived value without its source, `unobserved`
and not `false`) and `vcn_malformed_cidr` (a value that cannot be trusted, `unknown`) pin that. *implemented.* The engine does not
distinguish `unknown` from `unobserved` in the verdict (`classify_field`); only the observation digest does. A dedicated verdict
for `unknown` is *concept* (`conformance.rs` says so).

## The VCN slice, as pinned

Nine vectors, per-path verdicts and object verdict equal to the fixture's `expected/verdict.fields.json` (checked by
`fixtures/make_vcn_vectors.py --check`); the digests and the canonical string are what the engine computes. The plan digest is the
same for all nine (`sha256:91ddfbd8c4c8469c4e95c6a76ed9b8ac4fb9830f62b7f7d52a4802b1a41d5fd6`).

| Vector | Object | `observation_digest` |
|---|---|---|
| `vcn_conformant` | CONFORMANT | `sha256:610943f4497316182d62c2066546873e30a026f4ac33848b43171e4d6b567dec` |
| `vcn_drift` | DRIFT | `sha256:7a84b901a28ce036f6b4ba4c9acc68f79d53e9671b64a55217b9ef2795097421` |
| `vcn_field_absent` | DRIFT | `sha256:62287c8b3ebc9673d50cf62398c87d68f7186e93db26e4502efb00f3cca2bd24` |
| `vcn_desired_absent` | CONFORMANT | `sha256:62287c8b3ebc9673d50cf62398c87d68f7186e93db26e4502efb00f3cca2bd24` |
| `vcn_desired_absent_observed_present` | DRIFT | `sha256:610943f4497316182d62c2066546873e30a026f4ac33848b43171e4d6b567dec` |
| `vcn_partial_read` | INCOMPLETE | `sha256:4b419234ae459f6caa3ad08900d0f9299f92b7f855e79d05aeeabe4c5c64efcd` |
| `vcn_malformed_cidr` | INCOMPLETE | `sha256:462145a58a7f6cf0eda2c1389995a2930a9abe65734de3edb0bf0672fdaf1f4b` |
| `vcn_not_found` | INCOMPLETE | `sha256:7283447d63df630fae2347150c584468a3480e66e477e51f0ffb4b2970d61874` |
| `vcn_transport_error` | INCOMPLETE | `sha256:a7ca7c103862ba9fec9adde41ae2f75b316abcf67abd6d5e10ae4ba22521b298` |

## The open question: a planned path the intent does not declare

`vcn_desired_absent_observed_present`: `/dns_label` is in the plan, the intent declares nothing for it, OCI reports a value. The
engine's answer is `NOT_COMPARABLE` (the intent side resolves to null, null and a string are of different kinds), so the object is
`DRIFT`. The OCI module reads "absent in desired" as "unmanaged, do not touch". Both are consistent only if **the plan is derived
from the intent** (a path the intent does not declare is not planned) or the engine learns to tell an unmanaged field from a
declared one. This contract states the engine's current behaviour; changing either side is a decision for the owner. The existing
vector `oci_desired_absent_is_conformant` shows the *absent* half only (`OBSERVED_ABSENT`). *implemented* (behaviour),
*concept* (the resolution).

## Collections

`/tags/{key=env}/value` is the path of the `value` element of the element whose `key` is `env`, for a `map` collection keyed by
`key` (`collection::element_key`). *implemented*, as far as the vectors reach (two keys, one differing value).

* The element identities are the **sorted union of the identities on the intent side and on the observed side**
  (`plan.rs` `element_keys`; read from the source, **not** covered by a VCN vector). A tag that OCI reports and the intent does not
  name therefore becomes a path of its own, and unless the environment lists its coverage it is `unobserved`, which makes the
  object `INCOMPLETE`. The fixture lists coverage only for the intent's tag keys, so a real VCN with extra tags would need the
  environment to list those paths too (as `observed`; the intent side then resolves to null and the pair is `NOT_COMPARABLE`, the
  same question as below). Open for the owner. *concept.*
* A collection that is wholly unobserved is expressed through the coverage of each element path (`vcn_not_found`); there is no
  collection-level coverage. *implemented.*

## The receipt verifier

`verify_receipt` / `receipt.Verify` (`RECEIPT-VERIFIER.md`) is on `main` (PR #61, merged). It checks the form and the internal
consistency of a materialization receipt against an object and the authored input. **No receipt producer exists in this crate or,
as far as this repository shows, anywhere in CIC**, so the VCN slice has no receipt to verify. *implemented* (verifier),
*concept* (any receipt for this slice). The verdict and the receipt are independent artifacts (`VERDICT-SCHEMA.md`).

## What this slice does not exercise

* The vectors build objects with test-side helpers: every top-level field gets capability `implemented` and `Authored` evidence on
  the intent side. The registry's real objects (Access-wrapped values, `not_implemented` fields, schema defaults, derived values)
  are not built or compared here. *scaffold.*
* `secondary_cidr_blocks`, `display_name`, `compartment_id`, `lifecycle_state`, `ipv6_cidr_blocks` are not planned: the verdict says
  nothing about them (`fixtures/vcn-v1/not-planned.json`).
* Parse / Normalize / Resolve / Validate, name or OCID resolution, OCI-to-registry mapping: not in this crate, never will be.
* Evidence (ProofTrace) and anything that signs the verdict: the verdict is canonical bytes; signing and storing are not here.
