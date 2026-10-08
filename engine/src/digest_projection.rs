//! F5's two digest preimages (`docs/MATERIALIZATION-SPEC.md`), wired to
//! the real `ConformancePlan` (`plan.rs`) and the real per-path coverage/
//! value data `evaluate` records (`plan.rs`'s `ObjectVerdict::consumed`) —
//! the step F8's own doc comment named as separate and not yet done.
//!
//! This module does not decide anything new for `plan_digest_projection`/
//! `observation_digest_projection`: F5 already specified both
//! projections' exact `Value`-tree shape and ordering rule. It only
//! builds that shape from this engine's own already-existing types,
//! rather than inventing a fresh one.
//!
//! # `materialized_object_projection` -- the corrected-scope `IntentDigest` successor, and a new wire-layout decision
//!
//! F's own "Open" note (`MATERIALIZATION-SPEC.md`) named a real gap,
//! corrected 2026-10-08 after #39: Go's `SpecDigest`/`IntentDigest`
//! canonicalize an intent *after* running its own `ExpandSpec`/
//! `normalizeNumbers` (Normalize-stage work) -- this engine never runs
//! that step on anything, by the scope correction, so an "intent
//! digest" here cannot mean "digest of a freshly-normalized value" the
//! way it does in Go. It means something narrower and already
//! available: digest of an object the environment has *already*
//! materialized and handed over as a [`crate::materialized::
//! MaterializedObject`].
//!
//! **Unlike `plan_digest_projection`/`observation_digest_projection`,
//! this makes a new decision, not just a wiring one:** B7 committed
//! *that* capability/coverage/provenance are part of the materialized
//! semantic output `output_digest` must cover, but explicitly left
//! *how* they nest in the canonical byte tree to "C4/E2b's own open
//! layout questions" -- never actually closed since. The shape below
//! closes it, grounded directly in B1/B3's already-decided axis model,
//! not invented freely:
//!
//! ```text
//! Map { "fields": Seq[ Map{ "key", "capability", <axis>, "value"? } ] }
//! ```
//!
//! `<axis>` is never padded with a placeholder for an axis that
//! doesn't apply (`BOUNDARY.md`'s own anti-placeholder principle,
//! already applied by F5 to `value`/coverage above): an `Intent` field
//! carries `"provenance"` only; an `Observation` field carries
//! `"coverage"` only; a `DerivedObservation` field carries both --
//! B3's own "a derived state field carries both... simultaneously,"
//! with `"provenance"` always exactly `"derived"` there (the same fact
//! `FieldEvidence::DerivedObservation`'s own doc comment names: that
//! variant stores no separate `Provenance` field because it is always
//! exactly one value). `"key"`, not `"path"`: unlike F5's own
//! projections, which walk potentially-nested paths, a
//! `MaterializedObject`'s own fields are always exactly one flat
//! top-level key, by the Complete property's own definition -- naming
//! it `"path"` to match F5's vocabulary would imply a nesting capacity
//! this object does not have.

use crate::canonical::{digest, to_canonical_json};
use crate::error::Result;
use crate::materialized::{FieldEvidence, MaterializedObject, Provenance};
use crate::plan::{CollectionPlan, ConformancePlan, ConsumedField, FieldPlan};
use crate::reader::{Map, Value};
use std::collections::BTreeMap;

/// F5's `PlanDigestProjection`:
///
/// ```text
/// Map {
///   "scalars":     Seq[ Map{ "path": Str, "compare": Str } ],
///   "collections": Seq[ Map{ "path", "topology", "keys", "elements" } ]
/// }
/// ```
///
/// `scalars`/`collections`/every `elements` list sorted by `path`,
/// byte-wise; each collection's `keys` sorted byte-wise too — F5's own
/// ordering rule, reused here rather than re-decided.
#[must_use]
pub fn plan_digest_projection(plan: &ConformancePlan) -> Value {
    let mut scalars: Vec<&FieldPlan> = plan.scalars.iter().collect();
    scalars.sort_by(|a, b| a.path.cmp(&b.path));

    let mut collections: Vec<&CollectionPlan> = plan.collections.iter().collect();
    collections.sort_by(|a, b| a.path.cmp(&b.path));

    let mut m = Map::default();
    m.push(
        "scalars",
        Value::Seq(scalars.into_iter().map(field_plan_entry).collect()),
    );
    m.push(
        "collections",
        Value::Seq(collections.into_iter().map(collection_plan_entry).collect()),
    );
    Value::Map(m)
}

fn field_plan_entry(fp: &FieldPlan) -> Value {
    let mut m = Map::default();
    m.push("path", Value::Str(fp.path.clone()));
    m.push("compare", Value::Str(fp.compare.as_str().to_string()));
    Value::Map(m)
}

fn collection_plan_entry(cp: &CollectionPlan) -> Value {
    let mut keys = cp.collection.keys.clone();
    keys.sort();

    let mut elements: Vec<&FieldPlan> = cp.elements.iter().collect();
    elements.sort_by(|a, b| a.path.cmp(&b.path));

    let mut m = Map::default();
    m.push("path", Value::Str(cp.path.clone()));
    m.push(
        "topology",
        Value::Str(cp.collection.topology.as_str().to_string()),
    );
    m.push(
        "keys",
        Value::Seq(keys.into_iter().map(Value::Str).collect()),
    );
    m.push(
        "elements",
        Value::Seq(elements.into_iter().map(field_plan_entry).collect()),
    );
    Value::Map(m)
}

/// `conformance_plan_digest = digest(to_canonical_json(PlanDigestProjection))`
/// (F5, verbatim).
pub fn conformance_plan_digest(plan: &ConformancePlan) -> Result<String> {
    Ok(digest(&to_canonical_json(&plan_digest_projection(plan))?))
}

/// F5's `ObservationDigestProjection`:
///
/// ```text
/// Map { "fields": Seq[ Map{ "path", "coverage", "value"? } ] }
/// ```
///
/// `value` present **iff** `coverage == "observed"` (F5's own rule,
/// grounded in `classify_field_value`'s logic, not re-decided here) --
/// enforced by `ConsumedField`'s own type (review-caught on PR #29: an
/// earlier version read a `coverage`/`value: Option<Value>` pair and
/// trusted the caller to keep them consistent, which nothing actually
/// checked). `fields` sorted by `path`, byte-wise — `consumed` is
/// already a `BTreeMap<String, _>`, so its own iteration order already
/// *is* that sort; this function does not re-sort what's already sorted.
#[must_use]
pub fn observation_digest_projection(consumed: &BTreeMap<String, ConsumedField>) -> Value {
    let mut fields = Vec::with_capacity(consumed.len());
    for (path, cf) in consumed {
        let mut m = Map::default();
        m.push("path", Value::Str(path.clone()));
        m.push("coverage", Value::Str(cf.coverage().as_str().to_string()));
        if let Some(v) = cf.value() {
            m.push("value", v.clone());
        }
        fields.push(Value::Map(m));
    }
    let mut m = Map::default();
    m.push("fields", Value::Seq(fields));
    Value::Map(m)
}

/// `observation_digest = digest(to_canonical_json(ObservationDigestProjection))`
/// (F5, verbatim).
pub fn observation_digest(consumed: &BTreeMap<String, ConsumedField>) -> Result<String> {
    Ok(digest(&to_canonical_json(&observation_digest_projection(
        consumed,
    ))?))
}

/// `MaterializedObject`'s own canonical projection -- see the module
/// doc comment ("the corrected-scope `IntentDigest` successor") for the
/// shape and the reasoning behind it. `fields` sorted by `key`,
/// byte-wise -- `MaterializedObject::iter`'s own order already *is*
/// that sort (its internal `BTreeMap`), so this function does not
/// re-sort what's already sorted, the same reasoning
/// `observation_digest_projection` already applies to `consumed`.
#[must_use]
pub fn materialized_object_projection(obj: &MaterializedObject) -> Value {
    let mut fields = Vec::with_capacity(obj.len());
    for (key, field) in obj.iter() {
        let mut m = Map::default();
        m.push("key", Value::Str(key.clone()));
        m.push(
            "capability",
            Value::Str(field.capability.as_str().to_string()),
        );
        match &field.evidence {
            FieldEvidence::Intent(ie) => {
                m.push(
                    "provenance",
                    Value::Str(ie.provenance().as_str().to_string()),
                );
            }
            FieldEvidence::Observation(cf) => {
                m.push("coverage", Value::Str(cf.coverage().as_str().to_string()));
            }
            FieldEvidence::DerivedObservation(cf) => {
                m.push("coverage", Value::Str(cf.coverage().as_str().to_string()));
                m.push(
                    "provenance",
                    Value::Str(Provenance::Derived.as_str().to_string()),
                );
            }
        }
        if let Some(v) = field.value() {
            m.push("value", v.clone());
        }
        fields.push(Value::Map(m));
    }
    let mut m = Map::default();
    m.push("fields", Value::Seq(fields));
    Value::Map(m)
}

/// `materialized_object_digest = digest(to_canonical_json(
/// materialized_object_projection))` -- the corrected-scope
/// `IntentDigest` successor itself.
pub fn materialized_object_digest(obj: &MaterializedObject) -> Result<String> {
    Ok(digest(&to_canonical_json(
        &materialized_object_projection(obj),
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Collection, CollectionTopology};
    use crate::conformance::{CompareType, Coverage, Observation};
    use crate::materialized::{Capability, FieldEvidence, IntentEvidence, MaterializedField};
    use crate::plan::evaluate;
    use crate::reader::Map as RMap;
    use std::collections::BTreeSet;

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = RMap::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
    }

    /// Test-only: wraps a `Value::Map` fixture into a `MaterializedObject`
    /// -- see `plan.rs`'s own identical helper for why this is test-only,
    /// not a production conversion.
    fn object_from_value(
        v: Value,
        evidence_of: impl Fn(&str, Value) -> FieldEvidence,
    ) -> crate::materialized::MaterializedObject {
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
        crate::materialized::MaterializedObject::try_new(fields, &keys)
            .expect("a fixture's own key set is trivially complete against itself")
    }

    fn intent_object(v: Value) -> crate::materialized::MaterializedObject {
        object_from_value(v, |_, val| {
            FieldEvidence::Intent(IntentEvidence::Authored(Some(val)))
        })
    }

    /// Derives each field's REAL coverage from `obs` -- see `plan.rs`'s
    /// identical helper (review-caught on PR #41) for why this must match
    /// `Observation` by construction, not be wrapped as unconditionally
    /// `Observed`. A `Value::Seq` field is a collection container (none in
    /// this file's own fixtures today, but kept consistent with `plan.rs`
    /// rather than silently dropped) and stays unconditionally `Observed`
    /// for the same navigability reason `plan.rs`'s own doc comment gives.
    fn observed_object(v: Value, obs: &Observation) -> crate::materialized::MaterializedObject {
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

    // F5 names an exact shape, not just "scalar field paths and
    // comparators" -- this pins the actual field names/types down,
    // rather than only trusting the doc prose.
    #[test]
    fn plan_projection_has_f5s_exact_shape() {
        let plan = ConformancePlan {
            scalars: vec![
                FieldPlan {
                    path: "/b".into(),
                    compare: CompareType::Numeric,
                },
                FieldPlan {
                    path: "/a".into(),
                    compare: CompareType::Exact,
                },
            ],
            collections: vec![CollectionPlan {
                path: "/nics".into(),
                collection: Collection {
                    topology: CollectionTopology::Map,
                    keys: vec!["zone".into(), "name".into()],
                },
                elements: vec![FieldPlan {
                    path: "subnet".into(),
                    compare: CompareType::Exact,
                }],
            }],
        };
        let got = plan_digest_projection(&plan);
        let want = map(&[
            (
                "scalars",
                Value::Seq(vec![
                    map(&[
                        ("path", Value::Str("/a".into())),
                        ("compare", Value::Str("exact".into())),
                    ]),
                    map(&[
                        ("path", Value::Str("/b".into())),
                        ("compare", Value::Str("numeric".into())),
                    ]),
                ]),
            ),
            (
                "collections",
                Value::Seq(vec![map(&[
                    ("path", Value::Str("/nics".into())),
                    ("topology", Value::Str("map".into())),
                    (
                        "keys",
                        Value::Seq(vec![Value::Str("name".into()), Value::Str("zone".into())]),
                    ),
                    (
                        "elements",
                        Value::Seq(vec![map(&[
                            ("path", Value::Str("subnet".into())),
                            ("compare", Value::Str("exact".into())),
                        ])]),
                    ),
                ])]),
            ),
        ]);
        assert_eq!(got, want);
    }

    // Two plans that only differ in scalar/collection DECLARATION order
    // must digest identically -- the whole point of F5's own ordering
    // rule (section A's A6 would otherwise let Seq order leak into the
    // bytes).
    #[test]
    fn plan_digest_is_independent_of_declaration_order() {
        let a = ConformancePlan {
            scalars: vec![
                FieldPlan {
                    path: "/a".into(),
                    compare: CompareType::Exact,
                },
                FieldPlan {
                    path: "/b".into(),
                    compare: CompareType::Exact,
                },
            ],
            collections: vec![],
        };
        let b = ConformancePlan {
            scalars: vec![
                FieldPlan {
                    path: "/b".into(),
                    compare: CompareType::Exact,
                },
                FieldPlan {
                    path: "/a".into(),
                    compare: CompareType::Exact,
                },
            ],
            collections: vec![],
        };
        assert_eq!(
            conformance_plan_digest(&a).unwrap(),
            conformance_plan_digest(&b).unwrap()
        );
    }

    // value is present iff coverage == observed -- F5's own rule, checked
    // against all four B3 coverage values, not just the observed case.
    // Review-caught on PR #29: ConsumedField is now an enum specifically
    // so that an Observed-without-a-value (or non-Observed-with-a-value)
    // state can't even be constructed to test against -- there is no
    // longer a way to write the invalid case this test used to also
    // have to rule out.
    #[test]
    fn observation_projection_only_carries_a_value_when_observed() {
        let mut consumed = BTreeMap::new();
        consumed.insert(
            "/observed".to_string(),
            ConsumedField::Observed(Value::Str("v".into())),
        );
        consumed.insert("/absent".to_string(), ConsumedField::Absent);
        consumed.insert("/unobserved".to_string(), ConsumedField::Unobserved);
        consumed.insert("/unknown".to_string(), ConsumedField::Unknown);

        let got = observation_digest_projection(&consumed);
        let want = map(&[(
            "fields",
            Value::Seq(vec![
                map(&[
                    ("path", Value::Str("/absent".into())),
                    ("coverage", Value::Str("absent".into())),
                ]),
                map(&[
                    ("path", Value::Str("/observed".into())),
                    ("coverage", Value::Str("observed".into())),
                    ("value", Value::Str("v".into())),
                ]),
                map(&[
                    ("path", Value::Str("/unknown".into())),
                    ("coverage", Value::Str("unknown".into())),
                ]),
                map(&[
                    ("path", Value::Str("/unobserved".into())),
                    ("coverage", Value::Str("unobserved".into())),
                ]),
            ]),
        )]);
        assert_eq!(got, want);
    }

    // End-to-end: evaluate()'s own `consumed` output, fed straight into
    // observation_digest, actually distinguishes a run where the
    // observed VALUE differs from one where only coverage is identical --
    // the exact gap F5 widened Relay's envelope-only digest to close.
    #[test]
    fn observation_digest_distinguishes_different_observed_values() {
        let plan = ConformancePlan {
            scalars: vec![FieldPlan {
                path: "/shape".into(),
                compare: CompareType::Exact,
            }],
            collections: vec![],
        };
        let intent = map(&[("shape", Value::Str("E4.Flex".into()))]);
        let mut obs = Observation::new();
        obs.set("/shape", Coverage::Observed);

        let observed_a = map(&[("shape", Value::Str("E4.Flex".into()))]);
        let observed_b = map(&[("shape", Value::Str("E3.Flex".into()))]);

        let intent = intent_object(intent);
        let verdict_a = evaluate(&intent, &observed_object(observed_a, &obs), &obs, &plan);
        let verdict_b = evaluate(&intent, &observed_object(observed_b, &obs), &obs, &plan);

        // Same coverage on both sides -- Relay's own envelope-only digest
        // would have produced identical bytes for these two runs.
        assert_ne!(
            observation_digest(&verdict_a.consumed).unwrap(),
            observation_digest(&verdict_b.consumed).unwrap(),
            "observation_digest must differ when the observed VALUE \
             differs, even though coverage is identical on both sides"
        );
    }

    fn one_field_object(field: MaterializedField) -> crate::materialized::MaterializedObject {
        let mut fields = BTreeMap::new();
        fields.insert("x".to_string(), field);
        crate::materialized::MaterializedObject::try_new(fields, &keys(&["x"]))
            .expect("single key, trivially complete")
    }

    fn keys(ks: &[&str]) -> BTreeSet<String> {
        ks.iter().map(|s| s.to_string()).collect()
    }

    // Pins the exact per-kind shape the module doc comment decides, not
    // just trusting the doc prose -- the same discipline
    // plan_projection_has_f5s_exact_shape already applies to F5's own
    // projections.
    #[test]
    fn materialized_object_projection_has_the_decided_shape_per_evidence_kind() {
        let intent_field = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Intent(IntentEvidence::Authored(Some(Value::Str(
                "E4.Flex".into(),
            )))),
        };
        assert_eq!(
            materialized_object_projection(&one_field_object(intent_field)),
            map(&[(
                "fields",
                Value::Seq(vec![map(&[
                    ("key", Value::Str("x".into())),
                    ("capability", Value::Str("implemented".into())),
                    ("provenance", Value::Str("authored".into())),
                    ("value", Value::Str("E4.Flex".into())),
                ])]),
            )])
        );

        let observation_field = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Observed(Value::Int(16))),
        };
        assert_eq!(
            materialized_object_projection(&one_field_object(observation_field)),
            map(&[(
                "fields",
                Value::Seq(vec![map(&[
                    ("key", Value::Str("x".into())),
                    ("capability", Value::Str("implemented".into())),
                    ("coverage", Value::Str("observed".into())),
                    ("value", Value::Int(16)),
                ])]),
            )])
        );

        // B3: "a derived state field carries both... simultaneously" --
        // coverage AND provenance, provenance always exactly "derived".
        let derived_field = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::DerivedObservation(ConsumedField::Observed(Value::Str(
                "RUNNING".into(),
            ))),
        };
        assert_eq!(
            materialized_object_projection(&one_field_object(derived_field)),
            map(&[(
                "fields",
                Value::Seq(vec![map(&[
                    ("key", Value::Str("x".into())),
                    ("capability", Value::Str("implemented".into())),
                    ("coverage", Value::Str("observed".into())),
                    ("provenance", Value::Str("derived".into())),
                    ("value", Value::Str("RUNNING".into())),
                ])]),
            )])
        );
    }

    // BOUNDARY.md's own anti-placeholder principle: no value present
    // means no "value" key at all, never a null placeholder -- checked
    // for both the Observation/Absent case and B2's still-open
    // authored-absent case.
    #[test]
    fn materialized_object_projection_omits_value_rather_than_padding_with_null() {
        let absent_field = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Absent),
        };
        let got = materialized_object_projection(&one_field_object(absent_field));
        let Value::Map(m) = &got else { panic!() };
        let Value::Seq(fields) = m.get("fields").unwrap() else {
            panic!()
        };
        let Value::Map(entry) = &fields[0] else {
            panic!()
        };
        assert!(
            entry.get("value").is_none(),
            "an Absent field must have no \"value\" key at all, got {entry:?}"
        );

        let authored_absent_field = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Intent(IntentEvidence::Authored(None)),
        };
        let got = materialized_object_projection(&one_field_object(authored_absent_field));
        let Value::Map(m) = &got else { panic!() };
        let Value::Seq(fields) = m.get("fields").unwrap() else {
            panic!()
        };
        let Value::Map(entry) = &fields[0] else {
            panic!()
        };
        assert!(
            entry.get("value").is_none(),
            "an authored-absent field must have no \"value\" key at all, got {entry:?}"
        );
    }

    #[test]
    fn materialized_object_digest_distinguishes_different_values() {
        let a = one_field_object(MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Observed(Value::Int(16))),
        });
        let b = one_field_object(MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Observed(Value::Int(32))),
        });
        assert_ne!(
            materialized_object_digest(&a).unwrap(),
            materialized_object_digest(&b).unwrap(),
        );
    }
}
