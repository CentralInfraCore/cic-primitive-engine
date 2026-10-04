//! F5's two digest preimages (`docs/MATERIALIZATION-SPEC.md`), wired to
//! the real `ConformancePlan` (`plan.rs`) and the real per-path coverage/
//! value data `evaluate` records (`plan.rs`'s `ObjectVerdict::consumed`) —
//! the step F8's own doc comment named as separate and not yet done.
//!
//! This module does not decide anything new: F5 already specified both
//! projections' exact `Value`-tree shape and ordering rule. It only
//! builds that shape from this engine's own already-existing types,
//! rather than inventing a fresh one.

use crate::canonical::{digest, to_canonical_json};
use crate::conformance::Coverage;
use crate::error::Result;
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
/// grounded in `classify_field_value`'s logic, not re-decided here).
/// `fields` sorted by `path`, byte-wise — `consumed` is already a
/// `BTreeMap<String, _>`, so its own iteration order already *is* that
/// sort; this function does not re-sort what's already sorted.
#[must_use]
pub fn observation_digest_projection(consumed: &BTreeMap<String, ConsumedField>) -> Value {
    let mut fields = Vec::with_capacity(consumed.len());
    for (path, cf) in consumed {
        let mut m = Map::default();
        m.push("path", Value::Str(path.clone()));
        m.push("coverage", Value::Str(cf.coverage.as_str().to_string()));
        if cf.coverage == Coverage::Observed {
            if let Some(v) = &cf.value {
                m.push("value", v.clone());
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collection::{Collection, CollectionTopology};
    use crate::conformance::CompareType;
    use crate::plan::evaluate;
    use crate::reader::Map as RMap;

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = RMap::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
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
    #[test]
    fn observation_projection_only_carries_a_value_when_observed() {
        let mut consumed = BTreeMap::new();
        consumed.insert(
            "/observed".to_string(),
            ConsumedField {
                coverage: Coverage::Observed,
                value: Some(Value::Str("v".into())),
            },
        );
        consumed.insert(
            "/absent".to_string(),
            ConsumedField {
                coverage: Coverage::Absent,
                value: None,
            },
        );
        consumed.insert(
            "/unobserved".to_string(),
            ConsumedField {
                coverage: Coverage::Unobserved,
                value: None,
            },
        );
        consumed.insert(
            "/unknown".to_string(),
            ConsumedField {
                coverage: Coverage::Unknown,
                value: None,
            },
        );

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
        use crate::conformance::Observation;

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

        let verdict_a = evaluate(&intent, &observed_a, &obs, &plan);
        let verdict_b = evaluate(&intent, &observed_b, &obs, &plan);

        // Same coverage on both sides -- Relay's own envelope-only digest
        // would have produced identical bytes for these two runs.
        assert_ne!(
            observation_digest(&verdict_a.consumed).unwrap(),
            observation_digest(&verdict_b.consumed).unwrap(),
            "observation_digest must differ when the observed VALUE \
             differs, even though coverage is identical on both sides"
        );
    }
}
