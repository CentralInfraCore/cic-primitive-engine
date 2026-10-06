//! Runs the language-independent differential corpus in
//! `conformance/differential/` -- see that directory's own README for why
//! it is a second, JSON-only corpus layer, distinct from `../reader/`'s and
//! `../canonicalize/`'s YAML vectors: it exists to check whether the F6-F9
//! primitives agree across languages given an already-parsed value, not
//! whether the two languages' YAML readers agree with each other.

use std::fs;
use std::path::{Path, PathBuf};

use cic_primitive_engine::{
    classify_field_value, conformance_plan_digest, evaluate, observation_digest, reader,
    Capability, Collection, CollectionPlan, CollectionTopology, CompareType, ConformancePlan,
    ConsumedField, Coverage, FieldEvidence, FieldPlan, IntentEvidence, MaterializedField,
    MaterializedObject, Observation, Stage,
};
use reader::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Test-only: wraps a `Value::Map` fixture into a `MaterializedObject` --
/// same pattern as `plan.rs`'s and `digest_projection.rs`'s own identical
/// helpers. The Go peer this corpus cross-checks against has no
/// `MaterializedObject` equivalent either yet (`materialized.rs` is
/// Rust-only so far), so this corpus still compares both languages'
/// `evaluate` given the same intent/observed Values -- the wrapping only
/// satisfies Rust's own, now custody-checked, entry point.
fn object_from_value(
    v: &Value,
    evidence_of: impl Fn(&str, Value) -> FieldEvidence,
) -> MaterializedObject {
    let Value::Map(m) = v else {
        panic!("expected a mapping")
    };
    let mut fields = BTreeMap::new();
    let mut keys = BTreeSet::new();
    for k in m.keys() {
        let val = m.get(k).expect("key came from this map's own key list");
        keys.insert(k.to_string());
        fields.insert(
            k.to_string(),
            MaterializedField {
                capability: Capability::Implemented,
                evidence: evidence_of(k, val.clone()),
            },
        );
    }
    MaterializedObject::try_new(fields, &keys)
        .expect("a fixture's own key set is trivially complete against itself")
}

fn intent_object(v: &Value) -> MaterializedObject {
    object_from_value(v, |_, val| {
        FieldEvidence::Intent(IntentEvidence::Authored(Some(val)))
    })
}

/// Derives each field's REAL coverage from `obs` -- review-caught on PR
/// #41; see `plan.rs`'s identical helper for why this must match
/// `Observation` by construction, not be wrapped as unconditionally
/// `Observed`. A `Value::Seq` field is a collection container, detected
/// structurally rather than by name, and stays unconditionally `Observed`
/// for the navigability reason `plan.rs`'s own doc comment gives.
fn observed_object(v: &Value, obs: &Observation) -> MaterializedObject {
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

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .join("conformance")
        .join("differential")
}

struct Vector {
    name: String,
    input: Value,
    expected: Value,
}

fn load_vectors(group: &str) -> Vec<Vector> {
    let dir = corpus_root().join(group);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();

    dirs.into_iter()
        .map(|dir| {
            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            let input_bytes = fs::read(dir.join("input.json"))
                .unwrap_or_else(|e| panic!("{name}: input.json: {e}"));
            let expected_bytes = fs::read(dir.join("expected.json"))
                .unwrap_or_else(|e| panic!("{name}: expected.json: {e}"));
            let input = reader::parse(&input_bytes, Stage::Read, "$", "input.json")
                .unwrap_or_else(|e| panic!("{name}: input.json failed to parse: {e}"));
            let expected = reader::parse(&expected_bytes, Stage::Read, "$", "expected.json")
                .unwrap_or_else(|e| panic!("{name}: expected.json failed to parse: {e}"));
            Vector {
                name,
                input,
                expected,
            }
        })
        .collect()
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    match map_get(v, key) {
        Some(Value::Str(s)) => s,
        _ => panic!("expected string field {key:?}"),
    }
}

fn bool_field(v: &Value, key: &str) -> bool {
    match map_get(v, key) {
        Some(Value::Bool(b)) => *b,
        _ => panic!("expected bool field {key:?}"),
    }
}

fn map_get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Map(m) => m.get(key),
        _ => panic!("expected a mapping"),
    }
}

fn parse_coverage(s: &str) -> Coverage {
    match s {
        "observed" => Coverage::Observed,
        "absent" => Coverage::Absent,
        "unobserved" => Coverage::Unobserved,
        "unknown" => Coverage::Unknown,
        other => panic!("unknown coverage {other:?}"),
    }
}

fn parse_compare(s: &str) -> CompareType {
    match s {
        "exact" => CompareType::Exact,
        "numeric" => CompareType::Numeric,
        other => panic!("unknown compare type {other:?}"),
    }
}

fn parse_topology(s: &str) -> CollectionTopology {
    match s {
        "atomic" => CollectionTopology::Atomic,
        "set" => CollectionTopology::Set,
        "map" => CollectionTopology::Map,
        other => panic!("unknown topology {other:?}"),
    }
}

fn str_seq_field(v: &Value, key: &str) -> Vec<String> {
    match map_get(v, key) {
        Some(Value::Seq(items)) => items
            .iter()
            .map(|item| match item {
                Value::Str(s) => s.clone(),
                _ => panic!("expected every element of {key:?} to be a string"),
            })
            .collect(),
        _ => panic!("expected sequence field {key:?}"),
    }
}

/// `conformance/differential/comparator/` -- F6's `classify_field_value`.
/// Every vector's expected `FieldVerdict` is pinned by this engine's own
/// already-decided literal (`FieldVerdict::as_str`), so a failure here means
/// the comparator itself disagrees with its own documented contract, not
/// that this test's own expectations drifted from it.
#[test]
fn comparator_vectors() {
    let vectors = load_vectors("comparator");
    assert!(
        !vectors.is_empty(),
        "differential/comparator has no vectors -- an empty corpus trivially passes"
    );

    let mut failures = Vec::new();
    let mut seen_verdicts = std::collections::BTreeSet::new();
    for v in &vectors {
        let coverage = parse_coverage(str_field(&v.input, "coverage"));
        let intent_present = bool_field(&v.input, "intent_present");
        let intent = map_get(&v.input, "intent").unwrap_or(&Value::Null);
        let observed = map_get(&v.input, "observed").unwrap_or(&Value::Null);
        let compare = parse_compare(str_field(&v.input, "compare"));

        let got = classify_field_value(coverage, intent_present, intent, observed, compare);
        let want = str_field(&v.expected, "verdict");
        seen_verdicts.insert(want.to_string());

        if got.as_str() != want {
            failures.push(format!("{}: got {}, want {want}", v.name, got.as_str()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} comparator vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );

    // The harness's own invariant, mirroring conformance.rs's generic
    // corpus checks: a corpus that never exercises a given verdict can't
    // actually prove either language produces it correctly.
    for verdict in [
        "CONFORMANT",
        "DRIFT",
        "OBSERVED_ABSENT",
        "UNOBSERVED",
        "NOT_COMPARABLE",
    ] {
        assert!(
            seen_verdicts.contains(verdict),
            "no vector in differential/comparator exercises verdict {verdict}"
        );
    }
}

/// `conformance/differential/collection/` -- F7's `Collection::element_key`.
/// Includes the exact negative-zero vector F12's own cross-language bugfix
/// (`collection.rs`'s `go_float_display` used to fold `-0.0` to `"0"`) was
/// found and fixed against -- landing it here means that specific
/// regression now also fails loudly for either language on its own,
/// through the identical fixture, not only through each crate's own
/// hand-written unit test.
#[test]
fn collection_vectors() {
    let vectors = load_vectors("collection");
    assert!(
        !vectors.is_empty(),
        "differential/collection has no vectors -- an empty corpus trivially passes"
    );

    let mut failures = Vec::new();
    for v in &vectors {
        let topology = parse_topology(str_field(&v.input, "topology"));
        let keys = str_seq_field(&v.input, "keys");
        let elem = map_get(&v.input, "elem").unwrap_or(&Value::Null);
        let collection = Collection { topology, keys };

        let got = collection.element_key(elem);
        let want = str_field(&v.expected, "identity");

        if got != want {
            failures.push(format!("{}: got {got:?}, want {want:?}", v.name));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} collection vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );
}

fn build_observation(v: &Value) -> Observation {
    let mut obs = Observation::new();
    let Some(Value::Seq(items)) = map_get(v, "observation") else {
        panic!("expected sequence field \"observation\"");
    };
    for item in items {
        let path = str_field(item, "path");
        let coverage = parse_coverage(str_field(item, "coverage"));
        obs.set(path, coverage);
    }
    obs
}

fn build_field_plan(v: &Value) -> FieldPlan {
    FieldPlan {
        path: str_field(v, "path").to_string(),
        compare: parse_compare(str_field(v, "compare")),
    }
}

fn build_seq<T>(v: &Value, key: &str, f: impl Fn(&Value) -> T) -> Vec<T> {
    match map_get(v, key) {
        Some(Value::Seq(items)) => items.iter().map(f).collect(),
        _ => panic!("expected sequence field {key:?}"),
    }
}

fn build_plan(v: &Value) -> ConformancePlan {
    let plan_v = map_get(v, "plan").unwrap_or_else(|| panic!("expected field \"plan\""));
    let scalars = build_seq(plan_v, "scalars", build_field_plan);
    let collections = build_seq(plan_v, "collections", |cp| CollectionPlan {
        path: str_field(cp, "path").to_string(),
        collection: Collection {
            topology: parse_topology(str_field(cp, "topology")),
            keys: str_seq_field(cp, "keys"),
        },
        elements: build_seq(cp, "elements", build_field_plan),
    });
    ConformancePlan {
        scalars,
        collections,
    }
}

fn fields_from_value(v: &Value) -> BTreeMap<String, String> {
    let Some(Value::Map(m)) = map_get(v, "fields") else {
        panic!("expected mapping field \"fields\"");
    };
    let mut out = BTreeMap::new();
    for k in m.keys() {
        let val = m.get(k).expect("key came from this map's own key list");
        let Value::Str(s) = val else {
            panic!("expected string value for field {k:?}");
        };
        out.insert(k.to_string(), s.clone());
    }
    out
}

/// `conformance/differential/plan/` -- F8's `evaluate`. Includes the exact
/// multi-key-identity vector F8's own review round found and fixed
/// (`resolve_path`'s inherited Relay bug, PR #28) -- intent and observed
/// deliberately differ, so a silently-failed path resolution (both sides
/// falling back to the same missing-path default) would produce a false
/// `CONFORMANT` instead of the `DRIFT` this vector actually requires.
#[test]
fn plan_vectors() {
    let vectors = load_vectors("plan");
    assert!(
        !vectors.is_empty(),
        "differential/plan has no vectors -- an empty corpus trivially passes"
    );

    let mut failures = Vec::new();
    for v in &vectors {
        let intent = map_get(&v.input, "intent").unwrap_or(&Value::Null);
        let observed = map_get(&v.input, "observed").unwrap_or(&Value::Null);
        let obs = build_observation(&v.input);
        let plan = build_plan(&v.input);

        let verdict = evaluate(
            &intent_object(intent),
            &observed_object(observed, &obs),
            &obs,
            &plan,
        );

        let want_object = str_field(&v.expected, "object");
        if verdict.object.as_str() != want_object {
            failures.push(format!(
                "{}: object: got {}, want {want_object}",
                v.name,
                verdict.object.as_str()
            ));
        }

        let got_fields: BTreeMap<String, String> = verdict
            .fields
            .iter()
            .map(|(k, fv)| (k.clone(), fv.as_str().to_string()))
            .collect();
        let want_fields = fields_from_value(&v.expected);
        if got_fields != want_fields {
            failures.push(format!(
                "{}: fields mismatch\n    got:  {got_fields:?}\n    want: {want_fields:?}",
                v.name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} plan vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );
}

fn build_consumed(v: &Value) -> BTreeMap<String, ConsumedField> {
    let mut out = BTreeMap::new();
    let Some(Value::Seq(items)) = map_get(v, "consumed") else {
        panic!("expected sequence field \"consumed\"");
    };
    for item in items {
        let path = str_field(item, "path").to_string();
        let coverage = parse_coverage(str_field(item, "coverage"));
        let cf = match coverage {
            Coverage::Observed => {
                let value = map_get(item, "value").unwrap_or(&Value::Null);
                ConsumedField::Observed(value.clone())
            }
            Coverage::Absent => ConsumedField::Absent,
            Coverage::Unobserved => ConsumedField::Unobserved,
            Coverage::Unknown => ConsumedField::Unknown,
        };
        out.insert(path, cf);
    }
    out
}

/// `conformance/differential/digest/` -- F9's `conformance_plan_digest`/
/// `observation_digest`. Every expected digest here was computed once from
/// the real functions in BOTH languages and confirmed byte-for-byte
/// identical before being pinned -- not invented or hand-computed -- so a
/// failure here means one language's output actually changed, not that
/// the pinned value was ever a guess.
#[test]
fn digest_vectors() {
    let vectors = load_vectors("digest");
    assert!(
        !vectors.is_empty(),
        "differential/digest has no vectors -- an empty corpus trivially passes"
    );

    let mut failures = Vec::new();
    for v in &vectors {
        let want_digest = str_field(&v.expected, "digest");
        let got_digest = if map_get(&v.input, "plan").is_some() {
            let plan = build_plan(&v.input);
            conformance_plan_digest(&plan)
                .unwrap_or_else(|e| panic!("{}: conformance_plan_digest failed: {e}", v.name))
        } else if map_get(&v.input, "consumed").is_some() {
            let consumed = build_consumed(&v.input);
            observation_digest(&consumed)
                .unwrap_or_else(|e| panic!("{}: observation_digest failed: {e}", v.name))
        } else {
            panic!(
                "{}: input.json has neither \"plan\" nor \"consumed\"",
                v.name
            );
        };

        if got_digest != want_digest {
            failures.push(format!("{}: got {got_digest}, want {want_digest}", v.name));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} digest vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );
}
