//! The conformance/drift verdict artifact, assembled into the single shape
//! `docs/VERDICT-SCHEMA.md` commits to.
//!
//! `plan::evaluate` produces the semantic core (`object`, `fields`, and the
//! `consumed` map it recorded while classifying) and `digest_projection`
//! produces the two digests from a plan and from that `consumed` map. Nothing
//! bundled those into the one required five-field artifact until this module.
//!
//! This module decides nothing new:
//!
//! * `object` and `fields` are `evaluate`'s own result, carried over verbatim;
//! * `conformance_plan_digest` is [`conformance_plan_digest`] of the plan that
//!   was executed and `observation_digest` is [`observation_digest`] of the
//!   `consumed` map `evaluate` recorded -- the existing normative projections,
//!   called, never recomputed or re-derived here;
//! * `verdict_schema_version` is the bare integer `1` (`VERDICT-SCHEMA.md`).
//!
//! # What is checked before an artifact exists
//!
//! The schema says `fields` holds exactly one entry per path `evaluate`
//! classified, and that every field is required. [`build_verdict`] therefore
//! refuses an [`ObjectVerdict`] that cannot be such a result, rather than
//! digesting it into an artifact that would look authoritative:
//!
//! * the `fields` and `consumed` path sets must be identical (every classified
//!   path has its observation evidence, and no evidence is left over);
//! * every `plan.scalars` path must be in `fields`;
//! * `object` must be what [`aggregate`] gives for `fields`.
//!
//! A real `evaluate` result always passes; a result that was edited or
//! assembled by hand does not. Scope stays with the contract: signing the
//! artifact and an intent digest are explicitly "not decided" in the schema and
//! are not added here.

use std::collections::BTreeMap;

use crate::canonical::to_canonical_json;
use crate::conformance::FieldVerdict;
use crate::digest_projection::{conformance_plan_digest, observation_digest};
use crate::error::{code, Error, Result, Stage};
use crate::plan::{aggregate, ConformancePlan, ObjectConformance, ObjectVerdict};
use crate::reader::{Map, Value};

/// The only `verdict_schema_version` this module produces. Incremented on any
/// backward-incompatible change to the verdict contract (`VERDICT-SCHEMA.md`).
pub const VERDICT_SCHEMA_VERSION: i64 = 1;

/// The five required fields, in the order the schema lists them. There is no
/// optional member and no way to leave one out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceVerdict {
    pub verdict_schema_version: i64,
    pub object: ObjectConformance,
    pub fields: BTreeMap<String, FieldVerdict>,
    pub conformance_plan_digest: String,
    pub observation_digest: String,
}

/// Assembles the verdict artifact for `result`, the output of
/// [`crate::evaluate`] run against `plan`.
///
/// # Errors
/// `E_INCONSISTENT_VERDICT` (`R-VERDICT-CONSISTENT`) if `result` cannot be an
/// `evaluate` result for `plan` (see the module doc); a canonicalization error
/// from the digest functions if a consumed value holds a non-finite float.
pub fn build_verdict(plan: &ConformancePlan, result: &ObjectVerdict) -> Result<ConformanceVerdict> {
    check_consistent(plan, result)?;
    Ok(ConformanceVerdict {
        verdict_schema_version: VERDICT_SCHEMA_VERSION,
        object: result.object,
        fields: result.fields.clone(),
        conformance_plan_digest: conformance_plan_digest(plan)?,
        observation_digest: observation_digest(&result.consumed)?,
    })
}

fn inconsistent(path: &str, detail: String) -> Error {
    Error::new(
        code::INCONSISTENT_VERDICT,
        "R-VERDICT-CONSISTENT",
        Stage::Validate,
        path,
        detail,
    )
}

fn check_consistent(plan: &ConformancePlan, result: &ObjectVerdict) -> Result<()> {
    for path in result.fields.keys() {
        if !result.consumed.contains_key(path) {
            return Err(inconsistent(
                path,
                "classified path has no consumed observation evidence".to_string(),
            ));
        }
    }
    for path in result.consumed.keys() {
        if !result.fields.contains_key(path) {
            return Err(inconsistent(
                path,
                "consumed observation evidence for a path that was not classified".to_string(),
            ));
        }
    }
    for fp in &plan.scalars {
        if !result.fields.contains_key(&fp.path) {
            return Err(inconsistent(
                &fp.path,
                "planned scalar path is missing from fields".to_string(),
            ));
        }
    }
    let expected = aggregate(&result.fields);
    if expected != result.object {
        return Err(inconsistent(
            "$",
            format!(
                "object {} does not follow from fields (expected {})",
                result.object.as_str(),
                expected.as_str()
            ),
        ));
    }
    Ok(())
}

/// The artifact as a value tree: exactly the five required keys.
#[must_use]
pub fn verdict_value(v: &ConformanceVerdict) -> Value {
    let mut fields = Map::default();
    for (path, fv) in &v.fields {
        fields.push(path.clone(), Value::Str(fv.as_str().to_string()));
    }
    let mut m = Map::default();
    m.push(
        "verdict_schema_version",
        Value::Int(v.verdict_schema_version),
    );
    m.push("object", Value::Str(v.object.as_str().to_string()));
    m.push("fields", Value::Map(fields));
    m.push(
        "conformance_plan_digest",
        Value::Str(v.conformance_plan_digest.clone()),
    );
    m.push(
        "observation_digest",
        Value::Str(v.observation_digest.clone()),
    );
    Value::Map(m)
}

/// The artifact's canonical bytes (section A): the value tree of
/// [`verdict_value`] through [`to_canonical_json`].
///
/// # Errors
/// Never in practice (the tree holds only strings and one integer); the
/// signature stays fallible because canonicalization is.
pub fn verdict_canonical_bytes(v: &ConformanceVerdict) -> Result<Vec<u8>> {
    to_canonical_json(&verdict_value(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Collection, CollectionTopology};
    use crate::conformance::{CompareType, Coverage, Observation};
    use crate::materialized::{
        Capability, FieldEvidence, IntentEvidence, MaterializedField, MaterializedObject,
    };
    use crate::plan::{evaluate, ConsumedField, FieldPlan};
    use std::collections::BTreeSet;

    fn object(entries: &[(&str, FieldEvidence)]) -> MaterializedObject {
        let mut fields = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for (k, ev) in entries {
            keys.insert((*k).to_string());
            fields.insert(
                (*k).to_string(),
                MaterializedField {
                    capability: Capability::Implemented,
                    evidence: ev.clone(),
                },
            );
        }
        MaterializedObject::try_new(fields, &keys).expect("fixture key set is complete")
    }

    fn authored(v: Value) -> FieldEvidence {
        FieldEvidence::Intent(IntentEvidence::Authored(Some(v)))
    }

    fn observed(v: Value) -> FieldEvidence {
        FieldEvidence::Observation(ConsumedField::Observed(v))
    }

    fn sample() -> (ConformancePlan, ObjectVerdict) {
        let plan = ConformancePlan {
            scalars: vec![
                FieldPlan {
                    path: "/shape".to_string(),
                    compare: CompareType::Exact,
                },
                FieldPlan {
                    path: "/memory_gb".to_string(),
                    compare: CompareType::Numeric,
                },
            ],
            collections: vec![],
        };
        let intent = object(&[
            ("shape", authored(Value::Str("E4".into()))),
            ("memory_gb", authored(Value::Int(16))),
        ]);
        let observed_obj = object(&[
            ("shape", observed(Value::Str("E4".into()))),
            ("memory_gb", observed(Value::Float(32.0))),
        ]);
        let mut obs = Observation::new();
        obs.set("/shape", Coverage::Observed);
        obs.set("/memory_gb", Coverage::Observed);
        let result = evaluate(&intent, &observed_obj, &obs, &plan);
        (plan, result)
    }

    #[test]
    fn a_real_evaluate_result_builds_all_five_fields() {
        let (plan, result) = sample();
        let v = build_verdict(&plan, &result).expect("consistent");
        assert_eq!(v.verdict_schema_version, 1);
        assert_eq!(v.object, ObjectConformance::Drift);
        assert_eq!(v.fields.len(), 2);
        assert!(v.conformance_plan_digest.starts_with("sha256:"));
        assert!(v.observation_digest.starts_with("sha256:"));
    }

    #[test]
    fn the_digests_are_the_existing_projections_not_a_new_computation() {
        let (plan, result) = sample();
        let v = build_verdict(&plan, &result).unwrap();
        assert_eq!(
            v.conformance_plan_digest,
            conformance_plan_digest(&plan).unwrap()
        );
        assert_eq!(
            v.observation_digest,
            observation_digest(&result.consumed).unwrap()
        );
    }

    #[test]
    fn fields_hold_exactly_the_evaluated_paths() {
        let (plan, result) = sample();
        let v = build_verdict(&plan, &result).unwrap();
        let keys: Vec<&String> = v.fields.keys().collect();
        let evaluated: Vec<&String> = result.fields.keys().collect();
        assert_eq!(keys, evaluated);
    }

    #[test]
    fn the_value_tree_has_exactly_the_five_required_keys() {
        let (plan, result) = sample();
        let v = build_verdict(&plan, &result).unwrap();
        let Value::Map(m) = verdict_value(&v) else {
            panic!("a verdict is a mapping")
        };
        let mut keys: Vec<&str> = m.keys();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "conformance_plan_digest",
                "fields",
                "object",
                "observation_digest",
                "verdict_schema_version"
            ]
        );
        assert_eq!(m.get("verdict_schema_version"), Some(&Value::Int(1)));
    }

    #[test]
    fn canonical_bytes_are_sorted_and_stable() {
        let (plan, result) = sample();
        let v = build_verdict(&plan, &result).unwrap();
        let a = verdict_canonical_bytes(&v).unwrap();
        let b = verdict_canonical_bytes(&v).unwrap();
        assert_eq!(a, b);
        let text = String::from_utf8(a).unwrap();
        assert!(text.starts_with("{\"conformance_plan_digest\":\"sha256:"));
        assert!(text.ends_with("\"verdict_schema_version\":1}"));
    }

    #[test]
    fn a_classified_path_without_evidence_is_refused() {
        let (plan, mut result) = sample();
        result.consumed.remove("/shape");
        let e = build_verdict(&plan, &result).unwrap_err();
        assert_eq!(e.code, code::INCONSISTENT_VERDICT);
        assert_eq!(e.path, "/shape");
    }

    #[test]
    fn evidence_for_an_unclassified_path_is_refused() {
        let (plan, mut result) = sample();
        result
            .consumed
            .insert("/ghost".to_string(), ConsumedField::Absent);
        let e = build_verdict(&plan, &result).unwrap_err();
        assert_eq!(e.code, code::INCONSISTENT_VERDICT);
        assert_eq!(e.path, "/ghost");
    }

    #[test]
    fn a_planned_scalar_missing_from_fields_is_refused() {
        let (plan, mut result) = sample();
        result.fields.remove("/memory_gb");
        result.consumed.remove("/memory_gb");
        let e = build_verdict(&plan, &result).unwrap_err();
        assert_eq!(e.code, code::INCONSISTENT_VERDICT);
        assert_eq!(e.path, "/memory_gb");
    }

    #[test]
    fn an_object_verdict_that_does_not_follow_from_fields_is_refused() {
        let (plan, mut result) = sample();
        result.object = ObjectConformance::Conformant;
        let e = build_verdict(&plan, &result).unwrap_err();
        assert_eq!(e.code, code::INCONSISTENT_VERDICT);
        assert_eq!(e.path, "$");
    }

    #[test]
    fn a_collection_plan_does_not_change_the_checked_invariants() {
        let plan = ConformancePlan {
            scalars: vec![],
            collections: vec![crate::plan::CollectionPlan {
                path: "/nics".to_string(),
                collection: Collection {
                    topology: CollectionTopology::Map,
                    keys: vec!["name".to_string()],
                },
                elements: vec![],
            }],
        };
        let empty = ObjectVerdict {
            object: ObjectConformance::Conformant,
            fields: BTreeMap::new(),
            consumed: BTreeMap::new(),
        };
        // No element field was classified, so there is nothing to disagree about.
        assert!(build_verdict(&plan, &empty).is_ok());
    }
}
