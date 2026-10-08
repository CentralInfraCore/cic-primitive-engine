//! The object-level comparator walk — ported from `CIC-Relay`'s
//! `core/nexus/iac/conformance.go` (`ConformancePlan`/`FieldPlan`/
//! `CollectionPlan`/`Evaluate`/`aggregate`/`elementKeys`/`resolvePath`).
//! Closes F1's last open item: the per-field primitives (`conformance.rs`,
//! F6) and the element-identity primitive (`collection.rs`, F7) exist;
//! this is what drives them over a whole document.
//!
//! # A naming note, since Go's file layout doesn't map 1:1
//!
//! This engine's `conformance.rs` ports Go's `compare.go`/`observation.go`
//! (the per-field primitives) — **not** `conformance.go`, despite the
//! name. `conformance.go`'s own content — the object-level walk — lives
//! here, in `plan.rs`, named for what it actually is rather than for Go's
//! file that happens to hold it. Worth stating plainly so a future reader
//! comparing file names against Go's doesn't go looking for `Evaluate` in
//! the file literally named `conformance.rs`.
//!
//! # What this does not do
//!
//! Produce `conformance_plan_digest`/`observation_digest` themselves, or
//! Go's own `IntentDigest`. **Corrected 2026-10-08, after the scope
//! correction (#39):** an earlier version of this note blamed the gap on
//! "this engine's `Normalize` stage... doesn't exist yet" -- stale now
//! that Normalize is permanently the environment's job, never this
//! engine's own stage, per #39. The actual, current blocker is narrower:
//! nothing in this crate canonicalizes/digests a [`crate::materialized::
//! MaterializedObject`] itself yet (only the plan/consumed-observation
//! projections F5 specifies, below), and that is a self-contained task
//! with no missing precondition -- not something waiting on a stage
//! this engine will never build. [`evaluate`] does, however, now record
//! exactly what F5's two projections need as [`ObjectVerdict::consumed`]
//! — see `digest_projection.rs` for where that's turned into the actual
//! digests.
//!
//! # Wired to `MaterializedObject` at the root, not deeper
//!
//! [`evaluate`]'s `intent`/`observed` roots are [`crate::materialized::
//! MaterializedObject`], not a bare [`Value`] tree — `lib.rs`'s "next real
//! step" named after #40. Only the ROOT changed: a candidate must now pass
//! [`crate::materialized::MaterializedObject::try_new`]'s custody-boundary
//! check before it can reach this walker at all, closing the gap #39
//! named (a malformed candidate could previously reach `evaluate`
//! directly). Everything below the root — a field's own `.value()`,
//! collection elements, nested paths — is still a plain [`Value`] tree,
//! exactly as before: `MaterializedObject` is "exactly the schema's
//! keys," one [`crate::materialized::FieldEvidence`] per key, not a
//! recursive structure, so there is nothing deeper for it to replace yet.
//! **Review-caught on PR #41: a scalar path's coverage now has exactly
//! one authority, not two.** An earlier version of this wiring left
//! [`Observation`] as the sole coverage source even for top-level scalar
//! paths, while the observed root's own `FieldEvidence` already carried a
//! `Coverage` for the same path -- the two could disagree (demonstrated,
//! not hypothetical: this file's own `oci_unobserved` test exercised
//! exactly that disagreement), and `classify_at` silently trusted
//! `Observation` alone, which is exactly the kind of duplication this
//! crate has fought elsewhere (`ConsumedField`, `IntentEvidence`). Fixed
//! by [`scalar_coverage`] -- see its own doc comment for the rule and for
//! why it applies ONLY to scalar paths, not to collection elements or
//! collection containers: a collection's own top-level path carries no
//! real coverage of its own in this corpus (only its elements do), so
//! giving it the same authority rule would let a container's placeholder
//! evidence override every element's real coverage, a worse version of
//! the bug just fixed. That caveat is tied to the same recursion question
//! this module already named as undecided; scoping the fix to scalars
//! sidesteps it rather than silently deciding it.

use crate::collection::Collection;
use crate::conformance::{classify_field_value, CompareType, Coverage, FieldVerdict, Observation};
use crate::materialized::{FieldEvidence, MaterializedObject};
use crate::reader::Value;
use std::collections::{BTreeMap, BTreeSet};

/// One path's contribution to F5's `ObservationDigestProjection` —
/// `Coverage` paired with a value **exactly** when that pairing is
/// legitimate (grounded the same way F5 itself is: `observation.go`'s
/// `ClassifyFieldValue` never reads a value for any coverage state but
/// `Observed`, so there is nothing a non-`Observed` entry could
/// truthfully carry).
///
/// **Correction (review-caught on PR #29): an earlier version used
/// `{ coverage: Coverage, value: Option<Value> }`, which let
/// `Observed` pair with `None` (or any other coverage pair with
/// `Some`) — a state F5 forbids, constructible anyway, and silently
/// accepted by the projection builder rather than rejected.** For a
/// library whose entire job is proving a contract, an invalid state
/// that type-checks and digests without complaint is itself a defect,
/// independent of whether today's one caller (`evaluate`) happens to
/// always construct it correctly. Fixed by making the forbidden
/// pairing unrepresentable: an enum, not a struct of two independent
/// fields.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsumedField {
    Observed(Value),
    Absent,
    Unobserved,
    Unknown,
}

impl ConsumedField {
    #[must_use]
    pub fn coverage(&self) -> Coverage {
        match self {
            ConsumedField::Observed(_) => Coverage::Observed,
            ConsumedField::Absent => Coverage::Absent,
            ConsumedField::Unobserved => Coverage::Unobserved,
            ConsumedField::Unknown => Coverage::Unknown,
        }
    }

    #[must_use]
    pub fn value(&self) -> Option<&Value> {
        match self {
            ConsumedField::Observed(v) => Some(v),
            ConsumedField::Absent | ConsumedField::Unobserved | ConsumedField::Unknown => None,
        }
    }

    fn from_coverage(coverage: Coverage, observed_value: &Value) -> Self {
        match coverage {
            Coverage::Observed => ConsumedField::Observed(observed_value.clone()),
            Coverage::Absent => ConsumedField::Absent,
            Coverage::Unobserved => ConsumedField::Unobserved,
            Coverage::Unknown => ConsumedField::Unknown,
        }
    }
}

/// Ported from `conformance.go`'s `FieldPlan`: one comparable field path
/// and its comparator. `path` is canonical (`"/shape"`, or relative within
/// a collection element, e.g. `"subnet"`).
#[derive(Debug, Clone)]
pub struct FieldPlan {
    pub path: String,
    pub compare: CompareType,
}

/// Ported from `conformance.go`'s `CollectionPlan`: a topology-identified
/// list within the object, and the per-element field plans (paths relative
/// to the element).
#[derive(Debug, Clone)]
pub struct CollectionPlan {
    pub path: String,
    pub collection: Collection,
    pub elements: Vec<FieldPlan>,
}

/// Ported from `conformance.go`'s `ConformancePlan`: flat scalar fields
/// plus topology-identified collections.
#[derive(Debug, Clone, Default)]
pub struct ConformancePlan {
    pub scalars: Vec<FieldPlan>,
    pub collections: Vec<CollectionPlan>,
}

/// Ported from `conformance.go`'s `ObjectConformance`, identical
/// constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectConformance {
    /// Every planned field is `Conformant` or `ObservedAbsent`.
    Conformant,
    /// At least one field is `Drift` or `NotComparable`.
    Drift,
    /// No drift, but at least one planned field was `Unobserved`, so full
    /// conformance cannot be claimed.
    Incomplete,
}

impl ObjectConformance {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectConformance::Conformant => "CONFORMANT",
            ObjectConformance::Drift => "DRIFT",
            ObjectConformance::Incomplete => "INCOMPLETE",
        }
    }
}

/// Ported from `conformance.go`'s `ConformanceResult` -- minus
/// `IntentDigest`/`ObservationDigest` themselves (see `digest_projection.
/// rs` for those). `fields` is a `BTreeMap`, not Go's unordered
/// `map[string]FieldVerdict` -- deterministic iteration order (byte-wise
/// by path) rather than Go's none at all, consistent with every other
/// ordering decision this document has made explicit (F5, C4) rather than
/// left to chance. `consumed` is this engine's own addition, not Go's --
/// exactly the per-path coverage/value data F5's `ObservationDigestProjection`
/// needs, recorded at the only point that actually knows it (here), not
/// reconstructed later by re-walking the plan a second time.
#[derive(Debug, Clone)]
pub struct ObjectVerdict {
    pub object: ObjectConformance,
    pub fields: BTreeMap<String, FieldVerdict>,
    pub consumed: BTreeMap<String, ConsumedField>,
}

/// Ported from `conformance.go`'s `Evaluate`: the object-level conformance
/// verdict for `intent` against `observed`, under `obs`'s coverage and the
/// compiled `plan`. Only planned fields are compared; unplanned state
/// fields are intentionally ignored (observed state, not drift).
///
/// `intent`/`observed` are [`MaterializedObject`]s, not bare [`Value`]
/// trees — see the module doc comment ("Wired to `MaterializedObject` at
/// the root, not deeper") for exactly what that does and does not change.
#[must_use]
pub fn evaluate(
    intent: &MaterializedObject,
    observed: &MaterializedObject,
    obs: &Observation,
    plan: &ConformancePlan,
) -> ObjectVerdict {
    let mut fields = BTreeMap::new();
    let mut consumed = BTreeMap::new();

    for fp in &plan.scalars {
        let coverage = scalar_coverage(observed, &fp.path, obs);
        classify_at(
            &mut fields,
            &mut consumed,
            intent,
            observed,
            coverage,
            &fp.path,
            fp.compare,
        );
    }

    for cp in &plan.collections {
        for ek in element_keys(intent, observed, cp) {
            for ef in &cp.elements {
                let path = format!("{}/{{{ek}}}/{}", cp.path, ef.path);
                // Element-level paths stay Observation-authoritative --
                // see scalar_coverage's own doc comment for why only
                // scalar (single-segment) paths get the other rule.
                let coverage = obs.coverage(&path);
                classify_at(
                    &mut fields,
                    &mut consumed,
                    intent,
                    observed,
                    coverage,
                    &path,
                    ef.compare,
                );
            }
        }
    }

    ObjectVerdict {
        object: aggregate(&fields),
        fields,
        consumed,
    }
}

/// Review-caught on PR #41: with two coverage sources for the same
/// top-level path -- the observed root's own `FieldEvidence` and
/// `Observation` -- nothing previously stopped them from disagreeing, and
/// `classify_at` silently trusted `Observation` alone. For a scalar
/// (single-segment, non-collection) path, the observed root's own
/// evidence is now authoritative whenever it carries a coverage at all
/// (`FieldEvidence::Observation`/`DerivedObservation`); `Observation` is
/// consulted only as a fallback, for a path the root has no opinion on
/// (an `Intent`-evidence field, or a key `try_new` never saw -- which
/// `try_new`'s own Complete check already makes impossible for an actual
/// schema key, but this function does not assume that invariant reaches
/// it unbroken).
///
/// **Deliberately scalar-only, not applied to collection ELEMENT paths
/// (see `evaluate`'s own two call sites):** a collection's own top-level
/// path (e.g. `/network_interfaces`) is never an entry `Observation`
/// tracks at all in this corpus -- only its ELEMENTS are, each at its own
/// nested path -- so the container's top-level `FieldEvidence` exists
/// purely to make the list navigable (`element_keys`'s own
/// `resolve_path` call), not to classify the container itself. Giving a
/// collection path this same authority rule would make the container's
/// own placeholder evidence silently override every element's real,
/// per-element `Observation` coverage -- a worse version of the exact bug
/// this function fixes. Resolving that cleanly needs `MaterializedObject`
/// to carry per-element coverage directly, which is the recursion
/// question this crate has not decided; scoping this fix to scalars sidesteps
/// it rather than silently deciding it.
fn scalar_coverage(observed: &MaterializedObject, path: &str, obs: &Observation) -> Coverage {
    let key = path.trim_matches('/');
    if key.is_empty() || key.contains('/') || key.contains('{') {
        return obs.coverage(path);
    }
    match observed.get(key).map(|f| &f.evidence) {
        Some(FieldEvidence::Observation(cf) | FieldEvidence::DerivedObservation(cf)) => {
            cf.coverage()
        }
        _ => obs.coverage(path),
    }
}

fn classify_at(
    fields: &mut BTreeMap<String, FieldVerdict>,
    consumed: &mut BTreeMap<String, ConsumedField>,
    intent: &MaterializedObject,
    observed: &MaterializedObject,
    coverage: Coverage,
    path: &str,
    compare: CompareType,
) {
    let iv = resolve_path(intent, path);
    let ov = resolve_path(observed, path);
    let verdict = classify_field_value(
        coverage,
        iv.is_some(),
        iv.unwrap_or(&Value::Null),
        ov.unwrap_or(&Value::Null),
        compare,
    );
    fields.insert(path.to_string(), verdict);
    consumed.insert(
        path.to_string(),
        // F5's own rule, not invented here: a value is part of what the
        // comparator consumed iff coverage is Observed -- every other
        // state never reaches compare() at all (see classify_field_value,
        // F6) -- enforced by construction, not by convention, via
        // ConsumedField::from_coverage.
        ConsumedField::from_coverage(coverage, ov.unwrap_or(&Value::Null)),
    );
}

/// Ported from `conformance.go`'s `aggregate`: reduces per-field verdicts
/// to the object verdict. Precedence: `Drift`/`NotComparable` >
/// `Unobserved` > `Conformant`.
#[must_use]
pub fn aggregate(fields: &BTreeMap<String, FieldVerdict>) -> ObjectConformance {
    let mut has_drift = false;
    let mut has_unobserved = false;
    for v in fields.values() {
        match v {
            FieldVerdict::Drift | FieldVerdict::NotComparable => has_drift = true,
            FieldVerdict::Unobserved => has_unobserved = true,
            FieldVerdict::Conformant | FieldVerdict::ObservedAbsent => {}
        }
    }
    if has_drift {
        ObjectConformance::Drift
    } else if has_unobserved {
        ObjectConformance::Incomplete
    } else {
        ObjectConformance::Conformant
    }
}

/// Ported from `conformance.go`'s `elementKeys`: the sorted union of
/// element identities of a collection across the intent and observed
/// lists, keyed by the collection's topology. A `BTreeSet` gives the union
/// and the sort in one step, matching Go's `seen map` + `sort.Strings`.
fn element_keys(
    intent: &MaterializedObject,
    observed: &MaterializedObject,
    cp: &CollectionPlan,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for root in [intent, observed] {
        if let Some(Value::Seq(list)) = resolve_path(root, &cp.path) {
            for elem in list {
                let ek = cp.collection.element_key(elem);
                if !ek.is_empty() {
                    seen.insert(ek);
                }
            }
        }
    }
    seen.into_iter().collect()
}

/// Resolves `path`'s first segment against `root` itself — a
/// [`MaterializedObject`] key lookup, never a `{...}` collection-identity
/// segment (that syntax only ever selects an element *within* a field's
/// own value, never a schema key itself) — then delegates whatever
/// remains of `path` to [`resolve_in`], which does the actual walking
/// through the resulting [`Value`]. See the module doc comment ("Wired to
/// `MaterializedObject` at the root, not deeper") for why only this first
/// step changed.
fn resolve_path<'a>(root: &'a MaterializedObject, path: &str) -> Option<&'a Value> {
    let mut segs = path.trim_matches('/').splitn(2, '/');
    let first = segs.next()?;
    if first.is_empty() {
        return None;
    }
    let value = root.get(first)?.value()?;
    match segs.next() {
        Some(rest) if !rest.is_empty() => resolve_in(value, rest),
        _ => Some(value),
    }
}

/// Ported from `conformance.go`'s `resolvePath`: walks a canonical path
/// (`"/a/b"`, or `"/list/{key=val}/field"`, or, for a multi-key
/// collection, `"/list/{k1=v1,k2=v2}/field"`) through an already-resolved
/// `Value` tree -- everything in `path` past [`resolve_path`]'s own
/// top-level [`MaterializedObject`] lookup -- returning the value found,
/// or `None` if any segment along the way can't be resolved.
///
/// **Correction (review-caught on PR #28): Go's own `resolvePath` only
/// ever splits a `{...}` segment on the *first* `=`, via
/// `strings.Cut(seg, "=")`.** For a single-key identity that's
/// sufficient; for the multi-key identity `Collection::element_key`
/// already builds (`"name=nic-0,zone=eu"`, `collection.rs`, F7, tested
/// against `collection_test.go`'s own multi-key vector), it is not —
/// verified directly against `strings.Cut`: it returns `key="name"`,
/// `val="nic-0,zone=eu"`, which then looks for a field literally named
/// `"name"` whose *entire* value is the string `"nic-0,zone=eu"`,
/// matching nothing real. This is an inherited Relay bug, not a Go↔Rust
/// divergence — `conformance_test.go` never exercises a multi-key
/// `CollectionPlan`, so it was never caught there either — but porting it
/// here would leave this walker unable to resolve a path it generates
/// from its own, already-merged, multi-key-supporting `Collection` model.
/// Per A0's standing principle (`core/nexus/iac` is migration source and
/// tested reference material, not a contract this library must
/// reproduce bug-for-bug), fixed rather than ported: a `{...}` segment is
/// now parsed as the comma-separated `"k=v"` list `ElementKey` itself
/// builds, every pair matched against the candidate element's fields
/// via [`crate::collection::go_display`] (reused, not a second copy),
/// and **all** pairs must match. A single-key identity is simply the
/// one-constraint case of this, so the existing single-key path is
/// unaffected.
///
/// **A narrower, explicitly-named limitation inherited from the same
/// root cause, not fixed here because nothing exercises it either:**
/// Go's `resolvePath` also has no working case for a `TopologySet`
/// element at all (a bracketed segment with no `=` looks for a field
/// literally *named* the whole identity string, which almost never
/// exists) — this port doesn't invent a different, untested behavior
/// for that case; a `{...}` segment is only ever matched against `Map`
/// elements, same as Go.
///
/// `None` conflates "this segment doesn't exist" with "this segment
/// exists but isn't a value this path shape could be applied to" (e.g. a
/// `{...}` segment over a non-`Seq`), matching Go's own `resolvePath`,
/// which returns the identical `(nil, false)` for both.
fn resolve_in<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.trim_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        if let Some(inner) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let Value::Seq(list) = cur else {
                return None;
            };
            // Every "k=v" pair, comma-separated, matching ElementKey's
            // own construction -- all must match (see doc comment).
            let constraints: Vec<(&str, &str)> = inner
                .split(',')
                .map(|part| part.split_once('=').unwrap_or((part, "")))
                .collect();
            let found = list.iter().find(|elem| {
                let Value::Map(m) = elem else {
                    return false;
                };
                constraints.iter().all(|(key, val)| {
                    let display = match m.get(key) {
                        Some(v) => match crate::collection::go_display(v) {
                            Some(s) => s,
                            None => return false, // non-finite float -- never matches
                        },
                        None => "<nil>".to_string(),
                    };
                    display == *val
                })
            });
            cur = found?;
        } else {
            let Value::Map(m) = cur else {
                return None;
            };
            cur = m.get(seg)?;
        }
    }
    Some(cur)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::CollectionTopology;
    use crate::conformance::{CompareType, Coverage};
    use crate::materialized::{Capability, FieldEvidence, IntentEvidence, MaterializedField};
    use crate::reader::Map;

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = Map::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
    }

    /// Wraps a test fixture `Value::Map` into a [`MaterializedObject`] --
    /// a test-only convenience, not a production conversion (deciding how
    /// raw data becomes `FieldEvidence` is environment/Normalize work,
    /// out of this crate's scope; see the module doc comment). The
    /// fixture's own key set is trivially complete against itself, so
    /// `try_new` never fails here.
    fn object_from_value(
        v: Value,
        evidence_of: impl Fn(&str, Value) -> FieldEvidence,
    ) -> MaterializedObject {
        let Value::Map(m) = v else {
            panic!("test fixture must be a map")
        };
        let mut fields = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for (k, val) in m.0 {
            keys.insert(k.clone());
            let evidence = evidence_of(&k, val);
            fields.insert(
                k,
                MaterializedField {
                    capability: Capability::Implemented,
                    evidence,
                },
            );
        }
        MaterializedObject::try_new(fields, &keys)
            .expect("a fixture's own key set is trivially complete against itself")
    }

    fn intent_object(v: Value) -> MaterializedObject {
        object_from_value(v, |_, val| {
            FieldEvidence::Intent(IntentEvidence::Authored(Some(val)))
        })
    }

    /// Derives each scalar field's REAL coverage from `obs`, so the
    /// object's own claim and `Observation`'s claim can never disagree by
    /// construction -- mirroring, in the test fixtures, the same
    /// single-authority rule `scalar_coverage` (review-caught on PR #41)
    /// now enforces in production code. A `Value::Seq` field is a
    /// collection container, whose own top-level path `Observation` never
    /// tracks in this corpus (only its elements do, at a nested path) --
    /// detected structurally rather than by name, it stays unconditionally
    /// `Observed` so the walker can still navigate into it; needing this
    /// case at all is itself the reason `scalar_coverage` is scalar-only,
    /// not applied to collection paths.
    fn observed_object(v: Value, obs: &Observation) -> MaterializedObject {
        object_from_value(v, |k, val| {
            let cf = if matches!(val, Value::Seq(_)) {
                ConsumedField::Observed(val)
            } else {
                match obs.coverage(&format!("/{k}")) {
                    Coverage::Observed => ConsumedField::Observed(val),
                    Coverage::Absent => ConsumedField::Absent,
                    Coverage::Unobserved => ConsumedField::Unobserved,
                    Coverage::Unknown => ConsumedField::Unknown,
                }
            };
            FieldEvidence::Observation(cf)
        })
    }

    fn nic(name: &str, subnet_ref: &str, extra: &[(&str, Value)]) -> Value {
        let mut pairs: Vec<(&str, Value)> = vec![
            ("name", Value::Str(name.into())),
            (
                "subnet",
                map(&[
                    ("ref", Value::Str(subnet_ref.into())),
                    ("kind", Value::Str("Subnet".into())),
                ]),
            ),
        ];
        pairs.extend(extra.iter().cloned());
        map(&pairs)
    }

    // Mirrors conformance_test.go's ociIntent/ociObserved/ociPlan/
    // ociObservation fixtures -- the OCI vertical slice, ported field for
    // field, not invented.
    fn oci_intent() -> Value {
        map(&[
            ("shape", Value::Str("VM.Standard.E4.Flex".into())),
            ("memory_gb", Value::Int(16)),
            (
                "network_interfaces",
                Value::Seq(vec![
                    nic("nic-0", "prod-subnet-a", &[]),
                    nic("nic-1", "prod-subnet-b", &[]),
                ]),
            ),
        ])
    }

    fn oci_observed() -> Value {
        map(&[
            ("shape", Value::Str("VM.Standard.E4.Flex".into())),
            ("memory_gb", Value::Float(16.0)), // numeric-equal to 16, not exact-equal
            ("provider_id", Value::Str("ocid1.instance.oc1..aaaa".into())),
            ("lifecycle_state", Value::Str("RUNNING".into())),
            (
                "network_interfaces",
                Value::Seq(vec![
                    nic(
                        "nic-0",
                        "prod-subnet-a",
                        &[("private_ip", Value::Str("10.0.1.17".into()))],
                    ),
                    nic(
                        "nic-1",
                        "prod-subnet-b",
                        &[("private_ip", Value::Str("10.0.2.9".into()))],
                    ),
                ]),
            ),
        ])
    }

    fn oci_plan() -> ConformancePlan {
        ConformancePlan {
            scalars: vec![
                FieldPlan {
                    path: "/shape".into(),
                    compare: CompareType::Exact,
                },
                FieldPlan {
                    path: "/memory_gb".into(),
                    compare: CompareType::Numeric,
                },
            ],
            collections: vec![CollectionPlan {
                path: "/network_interfaces".into(),
                collection: Collection {
                    topology: CollectionTopology::Map,
                    keys: vec!["name".into()],
                },
                elements: vec![FieldPlan {
                    path: "subnet".into(),
                    compare: CompareType::Exact,
                }],
            }],
        }
    }

    fn oci_observation() -> Observation {
        let mut obs = Observation::new();
        for p in [
            "/shape",
            "/memory_gb",
            "/network_interfaces/{name=nic-0}/subnet",
            "/network_interfaces/{name=nic-1}/subnet",
        ] {
            obs.set(p, Coverage::Observed);
        }
        obs
    }

    // Mirrors TestEvaluate_OCI_Conformant.
    #[test]
    fn oci_conformant() {
        let obs = oci_observation();
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(oci_observed(), &obs),
            &obs,
            &oci_plan(),
        );
        assert_eq!(verdict.object, ObjectConformance::Conformant);
        for (path, v) in &verdict.fields {
            assert_eq!(*v, FieldVerdict::Conformant, "{path}");
        }
        // numeric normalization actually exercised: Int(16) vs Float(16.0).
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::Conformant)
        );
        // map-collection element paths resolved by identity.
        assert!(verdict
            .fields
            .contains_key("/network_interfaces/{name=nic-1}/subnet"));
    }

    // Mirrors TestEvaluate_OCI_ExtraStateFieldsAreNotDrift.
    #[test]
    fn oci_extra_state_fields_are_not_drift() {
        let obs = oci_observation();
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(oci_observed(), &obs),
            &obs,
            &oci_plan(),
        );
        for state_only in [
            "/provider_id",
            "/lifecycle_state",
            "/network_interfaces/{name=nic-0}/private_ip",
        ] {
            assert!(
                !verdict.fields.contains_key(state_only),
                "state-only field {state_only} leaked into the verdict set"
            );
        }
        assert_eq!(verdict.object, ObjectConformance::Conformant);
    }

    // Mirrors TestEvaluate_OCI_Drift.
    #[test]
    fn oci_drift() {
        let mut observed = oci_observed();
        let Value::Map(m) = &mut observed else {
            unreachable!()
        };
        let nics_idx =
            m.0.iter()
                .position(|(k, _)| k == "network_interfaces")
                .unwrap();
        let Value::Seq(nics) = &mut m.0[nics_idx].1 else {
            unreachable!()
        };
        nics[1] = nic("nic-1", "prod-subnet-WRONG", &[]);

        let obs = oci_observation();
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(observed, &obs),
            &obs,
            &oci_plan(),
        );
        assert_eq!(
            verdict
                .fields
                .get("/network_interfaces/{name=nic-1}/subnet"),
            Some(&FieldVerdict::Drift)
        );
        assert_eq!(verdict.object, ObjectConformance::Drift);
    }

    // Mirrors TestEvaluate_OCI_NotComparable.
    #[test]
    fn oci_not_comparable() {
        let mut observed = oci_observed();
        let Value::Map(m) = &mut observed else {
            unreachable!()
        };
        let idx = m.0.iter().position(|(k, _)| k == "memory_gb").unwrap();
        m.0[idx].1 = Value::Str("large".into());

        let obs = oci_observation();
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(observed, &obs),
            &obs,
            &oci_plan(),
        );
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::NotComparable)
        );
        assert_eq!(verdict.object, ObjectConformance::Drift);
    }

    // Mirrors TestEvaluate_OCI_Unobserved.
    #[test]
    fn oci_unobserved() {
        let mut obs = Observation::new();
        for p in [
            "/shape",
            "/network_interfaces/{name=nic-0}/subnet",
            "/network_interfaces/{name=nic-1}/subnet",
        ] {
            obs.set(p, Coverage::Observed);
        }
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(oci_observed(), &obs),
            &obs,
            &oci_plan(),
        );
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::Unobserved)
        );
        assert_eq!(verdict.object, ObjectConformance::Incomplete);
    }

    // Mirrors TestEvaluate_OCI_DesiredAbsentIsConformant.
    #[test]
    fn oci_desired_absent_is_conformant() {
        let mut plan = oci_plan();
        plan.scalars.push(FieldPlan {
            path: "/boot_volume".into(),
            compare: CompareType::Exact,
        });
        let mut obs = oci_observation();
        obs.set("/boot_volume", Coverage::Absent);

        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(oci_observed(), &obs),
            &obs,
            &plan,
        );
        assert_eq!(
            verdict.fields.get("/boot_volume"),
            Some(&FieldVerdict::ObservedAbsent)
        );
        assert_eq!(verdict.object, ObjectConformance::Conformant);
    }

    // Not in conformance_test.go -- this engine's own B2/B7-motivated
    // addition: `unknown` coverage (no Relay equivalent) must flow through
    // `evaluate` the same way `classify_field_value` (F6) already decided
    // it does, not get lost or mishandled at the walker layer.
    #[test]
    fn unknown_coverage_flows_through_to_unobserved_verdict() {
        let mut obs = oci_observation();
        obs.set("/memory_gb", Coverage::Unknown);
        let verdict = evaluate(
            &intent_object(oci_intent()),
            &observed_object(oci_observed(), &obs),
            &obs,
            &oci_plan(),
        );
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::Unobserved)
        );
    }

    // Review-caught on PR #41: directly proves scalar_coverage's single-
    // authority rule, not just that the existing OCI tests still pass by
    // construction. Builds a MaterializedObject whose own FieldEvidence
    // says /memory_gb is Absent, deliberately paired with an Observation
    // map that claims the SAME path is Observed, carrying a value --
    // exactly the two-authorities-disagree state review caught. Without
    // scalar_coverage's fix, classify_at would trust Observation alone
    // and call this Conformant/Drift against the (nonexistent) value the
    // object claims Absent. With it, the object's own claim wins.
    #[test]
    fn scalar_coverage_trusts_the_observed_root_over_a_disagreeing_observation_map() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "memory_gb".to_string(),
            MaterializedField {
                capability: Capability::Implemented,
                evidence: FieldEvidence::Observation(ConsumedField::Absent),
            },
        );
        let observed =
            MaterializedObject::try_new(fields, &["memory_gb".to_string()].into_iter().collect())
                .expect("single key, trivially complete");

        let mut intent_fields = BTreeMap::new();
        intent_fields.insert(
            "memory_gb".to_string(),
            MaterializedField {
                capability: Capability::Implemented,
                evidence: FieldEvidence::Intent(IntentEvidence::Authored(Some(Value::Int(16)))),
            },
        );
        let intent = MaterializedObject::try_new(
            intent_fields,
            &["memory_gb".to_string()].into_iter().collect(),
        )
        .expect("single key, trivially complete");

        let mut obs = Observation::new();
        obs.set("/memory_gb", Coverage::Observed); // disagrees with the root's own Absent claim

        let plan = ConformancePlan {
            scalars: vec![FieldPlan {
                path: "/memory_gb".into(),
                compare: CompareType::Numeric,
            }],
            collections: vec![],
        };

        let verdict = evaluate(&intent, &observed, &obs, &plan);
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::Drift),
            "intent wants a value, the object's own evidence says Absent -- \
             DRIFT, not a false CONFORMANT/UNOBSERVED taken from the \
             disagreeing Observation map"
        );
        assert_eq!(
            verdict.consumed.get("/memory_gb"),
            Some(&ConsumedField::Absent),
            "the recorded coverage for F5's own digest must also come from \
             the object, not from the disagreeing Observation map"
        );
    }

    // Review-caught on PR #28: not a port -- conformance_test.go never
    // exercises a multi-key CollectionPlan (its own ociPlan uses a single
    // key, "name"), so there is no existing Go vector to port against
    // (Go's own resolvePath has the identical bug this proves fixed, just
    // never caught by a test). Proves evaluate() can resolve a field path
    // it generated from its own multi-key ElementKey. Caught in review of
    // this very fix: an earlier version of this test used identical
    // intent/observed values, which passed even against the UNFIXED
    // bracket-matching code -- both sides silently fell back to
    // Value::Null when resolution failed, and Null trivially equals
    // Null, producing a false CONFORMANT that masked the exact failure
    // this test exists to catch. Intent and observed must differ for
    // this test to mean anything.
    #[test]
    fn multi_key_collection_identity_resolves_back_to_its_element() {
        let nic = |subnet: &str| {
            map(&[
                ("name", Value::Str("nic-0".into())),
                ("zone", Value::Str("eu".into())),
                ("subnet", Value::Str(subnet.into())),
            ])
        };
        let plan = ConformancePlan {
            scalars: vec![],
            collections: vec![CollectionPlan {
                path: "/nics".into(),
                collection: Collection {
                    topology: CollectionTopology::Map,
                    keys: vec!["name".into(), "zone".into()],
                },
                elements: vec![FieldPlan {
                    path: "subnet".into(),
                    compare: CompareType::Exact,
                }],
            }],
        };
        // Intent and observed deliberately DIFFER. This is the point: if
        // resolve_path's bracket-matching silently fails to find either
        // element (the bug this test exists to catch), both sides fall
        // back to Value::Null, which compares EQUAL to itself -- a false
        // CONFORMANT that would hide the exact failure this test needs to
        // surface. Only a correctly-resolved, genuinely different pair of
        // real values produces the DRIFT this test actually checks for.
        let intent = map(&[("nics", Value::Seq(vec![nic("prod-a")]))]);
        let observed = map(&[("nics", Value::Seq(vec![nic("prod-WRONG")]))]);
        let mut obs = Observation::new();
        obs.set("/nics/{name=nic-0,zone=eu}/subnet", Coverage::Observed);

        let verdict = evaluate(
            &intent_object(intent),
            &observed_object(observed, &obs),
            &obs,
            &plan,
        );
        assert_eq!(
            verdict.fields.get("/nics/{name=nic-0,zone=eu}/subnet"),
            Some(&FieldVerdict::Drift),
            "a multi-key identity must resolve back to the SAME real element on \
             both sides, not silently fall back to Null on both -- fields={:?}",
            verdict.fields
        );
        assert_eq!(verdict.object, ObjectConformance::Drift);
    }
}
