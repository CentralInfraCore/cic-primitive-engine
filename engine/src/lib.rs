//! The CIC primitive engine.
//!
//! # The contract
//!
//! **A module does not interpret CIC schema. This engine interprets it,
//! materializes it, and proves it; the module works only on the closed data
//! set.**
//!
//! That sentence is the reason this crate exists, and it is a stronger
//! constraint than any directory layout. Everything here either serves it or
//! does not belong here.
//!
//! # The object
//!
//! **This crate's actual subject is the materialized object a CIC
//! composition becomes.** The object carries exactly the schema's keys, no
//! more and no less (`PRIMITIVE-IR.md`'s own **Complete** property, enforced
//! by [`materialized::MaterializedObject::try_new`]). Per key,
//! [`materialized::MaterializedField`] holds `capability` (B1, always
//! present) plus [`materialized::FieldEvidence`] -- the closed set of
//! legitimate value/coverage/provenance combinations B3 actually describes.
//! **Correction, found while building the type:** an earlier version of
//! this doc comment described each key as "a value wrapped in the Access
//! atom's own structure (`value`/`access`/`modify`/`inherit`/
//! `default_injection`/`conformance`)" -- broader than what B3/B5 actually
//! decide. `access`/`modify`/`inherit`/`default_injection` are explicitly
//! **not** part of `MaterializedField` (B5: `default_injection` "is not a
//! second value held inside `MaterializedField`... it is computed at
//! response-construction time from the field's long-form descriptor," an
//! ACL/response-time concern the Relay/host owns, not this crate). See
//! `materialized`'s own module doc comment for the full reasoning.
//!
//! # Division of labor — scope correction
//!
//! **This crate does not perform domain/schema-semantic validation —
//! Shape/Role algebra, reference resolution, or default/derivation
//! decisions.** Those are facts the object's ENVIRONMENT establishes and
//! writes onto it — whatever upstream component already schema-validated
//! and supplemented the data before it reaches this object:
//!
//! ```text
//! environment                          this crate
//! ───────────                          ──────────
//! Parse       (typed composition)  ─┐
//! Normalize   (short/long form,     │  hands this crate a
//!              defaults applied)    │  candidate object
//! Resolve     (references, cycles)  │
//! Validate    (Shape/Role algebra) ─┘
//!                                      boundary check (candidate really
//!                                        is a MaterializedObject?)
//!                                      Canonicalize (one byte form, for digests)
//!                                      conformance/collection/plan/
//!                                      digest_projection (given two
//!                                        already-materialized states --
//!                                        intent and observed -- does one
//!                                        conform to the other, and proves it)
//! ```
//!
//! Scoping OUT domain-semantic validation does not scope out a narrower,
//! load-bearing obligation this crate still owns: whether the candidate
//! object it receives actually satisfies the structural/semantic-state
//! invariants — the `Complete` property, long-form fields, valid
//! capability/coverage/provenance values — that canonicalization and the
//! F6-F9 proof machinery assume. `docs/MATERIALIZATION-SPEC.md`'s B7 already
//! commits capability/coverage/provenance into the `output_digest`-protected
//! semantic claim; without a boundary check, that digest would only prove
//! "these bytes were canonicalized," not "a valid `MaterializedObject`
//! produced these bytes." F13/F14 (same spec) already established this
//! crate's own precedent for exactly this shape of problem: a module
//! validates untrusted input at its OWN boundary rather than trusting an
//! upstream stage (`ConformancePlan::validate`, `PlanDigestProjection`'s own
//! check) — the same principle applies one level up, at the
//! `MaterializedObject` boundary itself. That check does not exist yet,
//! because the type it protects does not exist yet (see "Status", below);
//! it is named here as an obligation this crate owes, not disclaimed as out
//! of scope.
//!
//! This corrects, rather than extends, the pipeline framing this crate's own
//! docs used to carry. `docs/MATERIALIZATION-SPEC.md` still describes
//! sections A through G at length as the historical record of how the
//! primitives' own semantics were decided -- that content stays correct
//! about what a capability/coverage/provenance fact, a comparator verdict,
//! or a canonical byte means. What changes here is narrower but load-bearing:
//! *who performs* `Parse`/`Normalize`/`Resolve`/`Validate`. It is the
//! environment, not this crate.
//!
//! # What this crate actually does
//!
//! - `reader` -- strict, schema-independent document reading (encoding,
//!   duplicate keys, aliases, key types) into a generic value tree, before
//!   any object exists yet. A general, mechanical processor: it does not
//!   know what a `cic-primitives` atom is.
//! - `canonical` -- one byte representation of a value tree, for digests
//!   (section A).
//! - `conformance`/`collection`/`plan`/`digest_projection` -- given two
//!   already-materialized values (intent, observed) plus a compiled
//!   comparison plan, answers whether one conforms to the other, and proves
//!   it with a digest (section F).
//!
//! # What does not belong in this crate
//!
//! Vault access, counter-signature policy, git and release workflow, domain
//! adapters, Kubernetes/OCI/network runtime logic, and authorization
//! decisions. **Also out of scope under the correction above:** domain/
//! schema-semantic validation, reference resolution, default/derivation
//! application, and short-form expansion -- the environment's job, before
//! data ever reaches this object. This does NOT include the structural
//! boundary check named under "Division of labor," above -- that stays
//! this crate's own obligation, just not yet built. `role`'s `expand_role`
//! (Role short/long form, P0.2) was
//! built before this correction and is exactly this kind of work; it stays
//! in the tree for now, named here rather than silently kept as if it still
//! belonged.
//!
//! A release verifier may call this engine to check the specs it carries,
//! but trust chain and provenance stay outside. Whether a primitive is
//! semantically valid and whether it came from someone you trust are
//! different questions, and merging them is how the archived
//! `cic-object-model` grew into a repository that had to be abandoned.
//!
//! # Status
//!
//! Early. `error` and `reader` are extracted and working. `canonical`
//! implements section A's byte format as a standalone primitive, over
//! whatever [`reader::Value`] tree it is handed. `conformance` implements
//! section F's per-field comparator primitive
//! (`compare`/`classify_field_value`) and `collection` implements its
//! topology/element-identity primitive (`Collection::element_key`); `plan`
//! drives both over a whole document (`evaluate`), and records the per-path
//! coverage/value data F5 needs; `digest_projection` turns that into F5's
//! own two digests (`conformance_plan_digest`/`observation_digest`) -- all
//! four cross-checked against an independent Go peer (`go/`) through a
//! shared differential corpus (`conformance/differential/`).
//!
//! **`materialized` exists** ([`materialized::MaterializedObject`]/
//! [`materialized::MaterializedField`]/[`materialized::FieldEvidence`]),
//! implementing B3's closed shape and the `try_new` custody-boundary check
//! named under "Division of labor," above. **`plan::evaluate` is wired to
//! it at the root:** `intent`/`observed` are now [`materialized::
//! MaterializedObject`]s, not bare [`reader::Value`] trees, so a candidate
//! must pass `try_new` before it can reach the comparator at all --
//! closing the actual gap #39 named, not just building the type that
//! could close it. Only the root changed; see `plan.rs`'s own doc comment
//! ("Wired to `MaterializedObject` at the root, not deeper") for exactly
//! what that does and does not mean -- `conformance`/`collection` still
//! take plain [`reader::Value`]s, correctly, since they operate on
//! already-resolved leaf values and collection elements, never on the
//! object itself. `role` predates the scope correction above and is not
//! wired to anything. Nothing here is a stable API, and no module should
//! depend on it as one.

pub mod canonical;
pub mod collection;
pub mod conformance;
pub mod digest_projection;
pub mod error;
pub mod materialized;
pub mod plan;
pub mod reader;
pub mod role;

pub use canonical::{digest, to_canonical_json};
pub use collection::{Collection, CollectionTopology};
pub use conformance::{
    classify_field, classify_field_value, compare, CompareType, Coverage, FieldVerdict, Observation,
};
pub use digest_projection::{
    conformance_plan_digest, observation_digest, observation_digest_projection,
    plan_digest_projection,
};
pub use error::{code, Error, Result, Stage};
pub use materialized::{
    Capability, FieldEvidence, IntentEvidence, MaterializedField, MaterializedObject, Provenance,
};
pub use plan::{
    evaluate, CollectionPlan, ConformancePlan, ConsumedField, FieldPlan, ObjectConformance,
    ObjectVerdict,
};
pub use role::{expand_role, Role};
