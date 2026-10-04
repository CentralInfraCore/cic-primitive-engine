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
//! Produce `conformance_plan_digest`/`observation_digest` (F5) or
//! `Go`'s own `IntentDigest`. F5 already decided these two digests commit
//! to the *executed plan* and the *full observation claim* respectively —
//! not simply `SpecDigest(intent)`, which is what Go's `IntentDigest`
//! actually is, and which this engine's own `Normalize` stage (not yet
//! built) would need to produce before an "intent digest" means the same
//! thing here it does in Go (Go's `SpecDigest` runs `ExpandSpec` and
//! `normalizeNumbers` first; this engine has no port of either yet).
//! Wiring F5's two projections to the plan/observation types this file
//! introduces is therefore its own, separate step — [`evaluate`] returns
//! the verdict only.

use crate::collection::Collection;
use crate::conformance::{classify_field_value, CompareType, FieldVerdict, Observation};
use crate::reader::Value;
use std::collections::{BTreeMap, BTreeSet};

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
/// `IntentDigest`/`ObservationDigest`. See this module's own doc comment
/// for why those are deliberately not here yet. `fields` is a `BTreeMap`,
/// not Go's unordered `map[string]FieldVerdict` -- deterministic iteration
/// order (byte-wise by path) rather than Go's none at all, consistent with
/// every other ordering decision this document has made explicit (F5, C4)
/// rather than left to chance.
#[derive(Debug, Clone)]
pub struct ObjectVerdict {
    pub object: ObjectConformance,
    pub fields: BTreeMap<String, FieldVerdict>,
}

/// Ported from `conformance.go`'s `Evaluate`: the object-level conformance
/// verdict for `intent` against `observed`, under `obs`'s coverage and the
/// compiled `plan`. Only planned fields are compared; unplanned state
/// fields are intentionally ignored (observed state, not drift).
#[must_use]
pub fn evaluate(
    intent: &Value,
    observed: &Value,
    obs: &Observation,
    plan: &ConformancePlan,
) -> ObjectVerdict {
    let mut fields = BTreeMap::new();

    for fp in &plan.scalars {
        classify_at(&mut fields, intent, observed, obs, &fp.path, fp.compare);
    }

    for cp in &plan.collections {
        for ek in element_keys(intent, observed, cp) {
            for ef in &cp.elements {
                let path = format!("{}/{{{ek}}}/{}", cp.path, ef.path);
                classify_at(&mut fields, intent, observed, obs, &path, ef.compare);
            }
        }
    }

    ObjectVerdict {
        object: aggregate(&fields),
        fields,
    }
}

fn classify_at(
    fields: &mut BTreeMap<String, FieldVerdict>,
    intent: &Value,
    observed: &Value,
    obs: &Observation,
    path: &str,
    compare: CompareType,
) {
    let iv = resolve_path(intent, path);
    let ov = resolve_path(observed, path);
    let verdict = classify_field_value(
        obs.coverage(path),
        iv.is_some(),
        iv.unwrap_or(&Value::Null),
        ov.unwrap_or(&Value::Null),
        compare,
    );
    fields.insert(path.to_string(), verdict);
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
fn element_keys(intent: &Value, observed: &Value, cp: &CollectionPlan) -> Vec<String> {
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

/// Ported from `conformance.go`'s `resolvePath`: walks a canonical path
/// (`"/a/b"`, or `"/list/{key=val}/field"`) into a nested `Value`,
/// returning the value found, or `None` if any segment along the way
/// can't be resolved. A `"{key=val}"` segment selects the list element
/// whose key field's [`crate::collection`]-style display equals `val` --
/// the exact same `fmt.Sprintf("%v", ...)` formatting `ElementKey` uses
/// (`go_display`, reused, not a second copy), including a missing key
/// field rendering as `"<nil>"`.
///
/// `None` conflates "this segment doesn't exist" with "this segment
/// exists but isn't a value this path shape could be applied to" (e.g. a
/// `{key=val}` segment over a non-`Seq`), matching Go's own
/// `resolvePath`, which returns the identical `(nil, false)` for both.
fn resolve_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = root;
    for seg in path.trim_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        if let Some(inner) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            let Value::Seq(list) = cur else {
                return None;
            };
            let (key, val) = inner.split_once('=').unwrap_or((inner, ""));
            let found = list.iter().find(|elem| {
                let Value::Map(m) = elem else {
                    return false;
                };
                let display = match m.get(key) {
                    Some(v) => match crate::collection::go_display(v) {
                        Some(s) => s,
                        None => return false, // non-finite float -- never matches
                    },
                    None => "<nil>".to_string(),
                };
                display == val
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
    use crate::reader::Map;

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = Map::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
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
        let verdict = evaluate(
            &oci_intent(),
            &oci_observed(),
            &oci_observation(),
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
        let verdict = evaluate(
            &oci_intent(),
            &oci_observed(),
            &oci_observation(),
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

        let verdict = evaluate(&oci_intent(), &observed, &oci_observation(), &oci_plan());
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

        let verdict = evaluate(&oci_intent(), &observed, &oci_observation(), &oci_plan());
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
        let verdict = evaluate(&oci_intent(), &oci_observed(), &obs, &oci_plan());
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

        let verdict = evaluate(&oci_intent(), &oci_observed(), &obs, &plan);
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
        let verdict = evaluate(&oci_intent(), &oci_observed(), &obs, &oci_plan());
        assert_eq!(
            verdict.fields.get("/memory_gb"),
            Some(&FieldVerdict::Unobserved)
        );
    }
}
