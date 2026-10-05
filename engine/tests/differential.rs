//! Runs the language-independent differential corpus in
//! `conformance/differential/` -- see that directory's own README for why
//! it is a second, JSON-only corpus layer, distinct from `../reader/`'s and
//! `../canonicalize/`'s YAML vectors: it exists to check whether the F6-F9
//! primitives agree across languages given an already-parsed value, not
//! whether the two languages' YAML readers agree with each other.

use std::fs;
use std::path::{Path, PathBuf};

use cic_primitive_engine::{
    classify_field_value, reader, Collection, CollectionTopology, CompareType, Coverage, Stage,
};
use reader::Value;

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
