//! `Value` is public, so a caller can build shapes the reader never produces.
//! Nothing reachable through the public API may panic on them or write bytes
//! that are not valid canonical JSON (#52).
//!
//! These cannot be conformance-corpus vectors: the corpus is YAML in, and the
//! reader cannot produce either shape.

use cic_primitive_engine::reader::{Map, Value};
use cic_primitive_engine::{
    classify_field_value, code, compare, digest, to_canonical_json, Collection, CollectionTopology,
    CompareType, Coverage, FieldVerdict, Stage,
};

fn big(s: &str) -> Value {
    Value::BigInt(s.to_string())
}

fn dup_map() -> Value {
    Value::Map(Map(vec![
        ("x".into(), Value::Int(1)),
        ("x".into(), Value::Int(2)),
    ]))
}

/// Not a signed digit run: each of these used to hit `expect(...)`.
const BAD_BIGINTS: &[&str] = &[
    "",
    "+",
    "-",
    "not-a-number",
    "12 3",
    "1e5",
    "1.5",
    "0x10",
    "--1",
    "٣",
];

#[test]
fn malformed_bigint_is_an_error_not_a_panic() {
    for s in BAD_BIGINTS {
        let err = to_canonical_json(&big(s)).expect_err(s);
        assert_eq!(err.code, code::MALFORMED_VALUE, "{s:?}");
        assert_eq!(err.rule, "R-CANON-BIGINT-DIGITS", "{s:?}");
        assert_eq!(err.stage, Stage::Canonicalize, "{s:?}");
        assert_eq!(err.path, "$", "{s:?}");
    }
}

#[test]
fn malformed_bigint_error_carries_the_nested_path() {
    let v = Value::Map(Map(vec![(
        "a".into(),
        Value::Seq(vec![Value::Int(1), big("nope")]),
    )]));
    let err = to_canonical_json(&v).expect_err("nested");
    assert_eq!(err.code, code::MALFORMED_VALUE);
    assert!(err.path.starts_with("$.a"), "path was {}", err.path);
}

#[test]
fn well_formed_bigints_still_canonicalize() {
    for (input, want) in [
        ("9223372036854775808", "9223372036854775808"),
        ("-9223372036854775809", "-9223372036854775809"),
        ("+000123", "123"),
        ("-0", "0"),
    ] {
        let got = to_canonical_json(&big(input)).expect(input);
        assert_eq!(String::from_utf8(got).unwrap(), want, "{input}");
    }
}

#[test]
fn duplicate_key_is_an_error_not_invalid_json() {
    let err = to_canonical_json(&dup_map()).expect_err("duplicate key");
    assert_eq!(err.code, code::MALFORMED_VALUE);
    assert_eq!(err.rule, "R-CANON-UNIQUE-KEYS");
    assert_eq!(err.stage, Stage::Canonicalize);
    assert_eq!(err.path, "$.x");
}

#[test]
fn duplicate_key_is_found_wherever_it_sits() {
    // Not adjacent in insertion order, and nested inside a sequence.
    let inner = Value::Map(Map(vec![
        ("a".into(), Value::Int(1)),
        ("b".into(), Value::Int(2)),
        ("a".into(), Value::Int(3)),
    ]));
    let v = Value::Map(Map(vec![(
        "k".into(),
        Value::Seq(vec![Value::Null, inner]),
    )]));
    let err = to_canonical_json(&v).expect_err("nested duplicate");
    assert_eq!(err.rule, "R-CANON-UNIQUE-KEYS");
    assert!(err.path.ends_with(".a"), "path was {}", err.path);
}

#[test]
fn unique_keys_are_still_sorted_bytewise() {
    let v = Value::Map(Map(vec![
        ("b".into(), Value::Int(2)),
        ("a".into(), Value::Int(1)),
        ("B".into(), Value::Int(0)),
    ]));
    let got = String::from_utf8(to_canonical_json(&v).unwrap()).unwrap();
    assert_eq!(got, r#"{"B":0,"a":1,"b":2}"#);
}

#[test]
fn digest_input_can_only_come_from_valid_canonical_json() {
    // `digest` takes bytes, so the guard is that no bytes are produced at all.
    assert!(to_canonical_json(&dup_map()).is_err());
    assert!(to_canonical_json(&big("x")).is_err());
    assert!(digest(b"{}").starts_with("sha256:"));
}

#[test]
fn compare_does_not_panic_on_a_malformed_bigint() {
    let good = Value::Int(5);
    for s in BAD_BIGINTS {
        let bad = big(s);
        // Exact: a malformed number has no canonical form -> not comparable.
        assert_eq!(
            compare(&bad, &good, CompareType::Exact),
            (false, false),
            "{s:?}"
        );
        assert_eq!(
            compare(&good, &bad, CompareType::Exact),
            (false, false),
            "{s:?}"
        );
        assert_eq!(
            compare(&bad, &bad, CompareType::Exact),
            (false, false),
            "{s:?}"
        );
        // Numeric already failed closed (BigInt::from_str).
        assert_eq!(
            compare(&bad, &good, CompareType::Numeric),
            (false, false),
            "{s:?}"
        );
    }
}

#[test]
fn classification_does_not_panic_on_hand_built_values() {
    let v = classify_field_value(
        Coverage::Observed,
        true,
        &big("nope"),
        &Value::Int(1),
        CompareType::Exact,
    );
    assert_eq!(v, FieldVerdict::NotComparable);
    // Two equal duplicate-key maps are not "conformant": they have no canonical form.
    let v = classify_field_value(
        Coverage::Observed,
        true,
        &dup_map(),
        &dup_map(),
        CompareType::Exact,
    );
    assert_ne!(v, FieldVerdict::Conformant);
}

#[test]
fn set_element_key_does_not_panic_on_hand_built_values() {
    let set = Collection {
        topology: CollectionTopology::Set,
        keys: vec![],
    };
    // No display form -> the empty identity, the same fallback a non-finite float gets.
    assert_eq!(set.element_key(&big("nope")), "");
    assert_eq!(set.element_key(&dup_map()), "");
    assert_eq!(set.element_key(&Value::Seq(vec![dup_map()])), "");
}
