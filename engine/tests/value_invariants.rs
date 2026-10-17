//! `Value` is public, so what a caller can build is part of the engine's
//! contract. The shapes the reader never produces -- a `BigInt` that is not a
//! digit run, a `Map` with a repeated key -- are refused by the constructors
//! (#53; #52 had made the canonicalizer fail closed on them first). Everything
//! below goes through the public API only.
//!
//! "Cannot be built around the constructor" is pinned by the `compile_fail`
//! doctests on `BigIntValue` and `Map` in `reader.rs`. None of this can be a
//! conformance-corpus vector: the corpus is YAML in, and the reader cannot
//! produce these shapes.

use cic_primitive_engine::reader::{self, BigIntValue, Map, Value};
use cic_primitive_engine::{
    classify_field_value, code, compare, to_canonical_json, Collection, CollectionTopology,
    CompareType, Coverage, FieldVerdict, Stage,
};

fn big(s: &str) -> Value {
    Value::BigInt(BigIntValue::try_new(s).expect(s))
}

fn pairs(kv: &[(&str, i64)]) -> Vec<(String, Value)> {
    kv.iter()
        .map(|(k, v)| ((*k).to_string(), Value::Int(*v)))
        .collect()
}

fn canon(v: &Value) -> String {
    String::from_utf8(to_canonical_json(v).expect("canonical")).unwrap()
}

/// Not a signed digit run: each of these used to reach an `expect(...)`.
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
    "+-1",
    "٣",
    "1\n",
];

#[test]
fn bigint_constructor_rejects_anything_but_a_signed_digit_run() {
    for s in BAD_BIGINTS {
        let err = BigIntValue::try_new(*s).expect_err(s);
        assert_eq!(err.code, code::MALFORMED_VALUE, "{s:?}");
        assert_eq!(err.rule, "R-VALUE-BIGINT-DIGITS", "{s:?}");
        assert_eq!(err.stage, Stage::Read, "{s:?}");
    }
}

#[test]
fn bigint_constructor_accepts_and_normalizes_digit_runs() {
    for (input, want) in [
        ("9223372036854775808", "9223372036854775808"),
        ("-9223372036854775809", "-9223372036854775809"),
        ("+000123", "123"),
        ("-000123", "-123"),
        ("-0", "0"),
        ("+0", "0"),
        ("0000", "0"),
    ] {
        let n = BigIntValue::try_new(input).expect(input);
        assert_eq!(n.as_str(), input, "as_str keeps the text as given");
        assert_eq!(n.canonical(), want, "{input}");
        assert_eq!(canon(&Value::BigInt(n)), want, "{input}");
    }
}

#[test]
fn from_pairs_rejects_a_repeated_key_wherever_it_sits() {
    for kv in [
        &[("x", 1), ("x", 2)][..],
        &[("a", 1), ("b", 2), ("a", 3)][..], // not adjacent in insertion order
        &[("a", 1), ("b", 2), ("c", 3), ("c", 4), ("b", 5)][..],
    ] {
        let err = Map::from_pairs(pairs(kv)).expect_err("duplicate");
        assert_eq!(err.code, code::MALFORMED_VALUE);
        assert_eq!(err.rule, "R-VALUE-UNIQUE-KEYS");
        assert_eq!(err.stage, Stage::Read);
        assert!(err.path.starts_with("$."), "path was {}", err.path);
    }
}

#[test]
fn from_pairs_keeps_unique_keys_in_insertion_order() {
    let m = Map::from_pairs(pairs(&[("b", 2), ("a", 1), ("B", 0)])).unwrap();
    assert_eq!(m.keys(), ["b", "a", "B"]);
    assert_eq!(m.len(), 3);
    // Canonical output is sorted bytewise regardless of insertion order.
    assert_eq!(canon(&Value::Map(m)), r#"{"B":0,"a":1,"b":2}"#);
    assert!(Map::from_pairs(Vec::new()).unwrap().is_empty());
}

#[test]
fn insert_replaces_so_uniqueness_survives_mutation() {
    let mut m = Map::new();
    m.insert("x", Value::Int(1));
    m.insert("x", Value::Int(2));
    m.insert("y", Value::Int(3));
    assert_eq!(m.len(), 2);
    assert_eq!(canon(&Value::Map(m)), r#"{"x":2,"y":3}"#);
}

#[test]
fn reader_output_satisfies_both_invariants() {
    let doc = "big: 9223372036854775808\nneg: -00000000000000000000000000000000000001\nn:\n  a: 1\n  b: 2\n";
    let v = reader::parse(doc.as_bytes(), Stage::Read, "$", "test").expect("parse");
    assert_eq!(
        canon(&v),
        r#"{"big":9223372036854775808,"n":{"a":1,"b":2},"neg":-1}"#
    );
    // A duplicate key never reaches the type: the reader refuses the document.
    let dup = reader::parse(b"x: 1\nx: 2\n", Stage::Read, "$", "test").expect_err("duplicate");
    assert_eq!(dup.code, code::MALFORMED_DOCUMENT);
}

#[test]
fn comparison_and_classification_handle_every_constructible_bigint() {
    let five = Value::Int(5);
    let huge = big("9223372036854775808");
    assert_eq!(compare(&huge, &huge, CompareType::Exact), (true, true));
    assert_eq!(
        compare(&big("+0009223372036854775808"), &huge, CompareType::Exact),
        (true, true)
    );
    assert_eq!(compare(&huge, &five, CompareType::Exact), (false, true));
    assert_eq!(compare(&huge, &five, CompareType::Numeric), (false, true));
    assert_eq!(
        classify_field_value(Coverage::Observed, true, &huge, &huge, CompareType::Exact),
        FieldVerdict::Conformant
    );
}

#[test]
fn set_element_key_uses_the_canonical_bigint_form() {
    let set = Collection {
        topology: CollectionTopology::Set,
        keys: vec![],
    };
    assert_eq!(set.element_key(&big("+0001")), "1");
    assert_eq!(set.element_key(&big("-0")), "0");
}
