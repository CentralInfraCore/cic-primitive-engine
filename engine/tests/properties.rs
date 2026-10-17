//! Property tests over generated `Value`s and generated input (#54).
//!
//! The corpus-based suites pin cases someone thought of; these check the
//! promises over inputs nobody wrote down: the canonicalizer never panics and
//! writes valid, sorted, unique-key JSON; it does not depend on insertion
//! order; the digest is a pure function of the canonical bytes; the
//! constructors refuse exactly the shapes they promise to refuse; the reader
//! never panics on arbitrary bytes.
//!
//! `export_cross_language_corpus` additionally writes a fixed-seed corpus for
//! the Go peer (`go/canonical/crosslang_test.go`) so Rust and Go are compared
//! on generated input, not only on the hand-written differential vectors.
//!
//! Case count: `PROPTEST_CASES` (the scheduled workflow raises it).

use cic_primitive_engine::reader::{self, BigIntValue, Map, Value};
use cic_primitive_engine::{compare, digest, to_canonical_json, CompareType, Stage};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, TestRunner};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write as _;

// ---------------------------------------------------------------- generators

/// Text for a BigInt: mostly digit runs, sometimes arbitrary garbage. It goes
/// through the real constructor, so garbage only becomes a BigInt if the
/// constructor accepts it -- a constructor that accepted too much would let it
/// through and the canonical-form properties below would fail. (Rejected text
/// maps to `0` instead of being filtered out, which keeps proptest's reject
/// budget out of play at large case counts.)
fn arb_bigint() -> impl Strategy<Value = Value> {
    prop_oneof![3 => "[+-]?[0-9]{1,40}", 1 => any::<String>()].prop_map(|s| {
        Value::BigInt(
            BigIntValue::try_new(s)
                .unwrap_or_else(|_| BigIntValue::try_new("0").expect("0 is a digit run")),
        )
    })
}

fn arb_leaf() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Int),
        arb_bigint(),
        // Every finite class (no NaN / infinity: those are a separate,
        // documented rejection, covered by the corpus).
        (prop::num::f64::POSITIVE
            | prop::num::f64::NEGATIVE
            | prop::num::f64::NORMAL
            | prop::num::f64::SUBNORMAL
            | prop::num::f64::ZERO)
            .prop_map(Value::Float),
        any::<String>().prop_map(Value::Str),
    ]
}

/// A finite value tree. Map keys come from a `BTreeMap`, so they are unique;
/// the uniqueness guarantee itself is tested separately.
fn arb_value() -> impl Strategy<Value = Value> {
    arb_leaf().prop_recursive(4, 48, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..5).prop_map(Value::Seq),
            prop::collection::btree_map(any::<String>(), inner, 0..5).prop_map(|m| {
                Value::Map(Map::from_pairs(m).expect("BTreeMap keys are unique"))
            }),
        ]
    })
}

/// The same tree with every mapping's insertion order changed (reversed, then
/// rotated), so canonical output must not depend on it.
fn reorder(v: &Value) -> Value {
    match v {
        Value::Seq(items) => Value::Seq(items.iter().map(reorder).collect()),
        Value::Map(m) => {
            let mut pairs: Vec<(String, Value)> =
                m.iter().map(|(k, v)| (k.to_string(), reorder(v))).collect();
            pairs.reverse();
            if !pairs.is_empty() {
                pairs.rotate_left(1);
            }
            Value::Map(Map::from_pairs(pairs).expect("same unique keys"))
        }
        other => other.clone(),
    }
}

/// A document holding `v` under one key. A bare top-level scalar is not a
/// meaningful document for the reader (`null`/`~`/empty all read as an empty
/// mapping, by design), so round trips go through a mapping.
fn wrapped(v: &Value) -> Value {
    Value::Map(Map::from_pairs([("v".to_string(), v.clone())]).expect("one key"))
}

/// JSON text for a generated value (strict JSON, so both readers accept it).
fn to_json_text(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::BigInt(n) => n.canonical(),
        Value::Float(f) => format!("{f:?}"),
        Value::Str(s) => serde_json::to_string(s).unwrap(),
        Value::Seq(items) => format!(
            "[{}]",
            items.iter().map(to_json_text).collect::<Vec<_>>().join(",")
        ),
        Value::Map(m) => format!(
            "{{{}}}",
            m.iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), to_json_text(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

// ------------------------------------------------- independent JSON checking

/// Deserializes any JSON, failing on an object whose keys are not strictly
/// ascending by bytes (which also rules out a repeated key). `serde_json`'s
/// own `Value` would silently drop a duplicate, so it cannot be the check.
struct StrictSorted;

impl<'de> Deserialize<'de> for StrictSorted {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StrictSorted;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with strictly ascending object keys")
            }
            fn visit_unit<E>(self) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_bool<E>(self, _: bool) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_i64<E>(self, _: i64) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_u64<E>(self, _: u64) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_f64<E>(self, _: f64) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_str<E>(self, _: &str) -> Result<StrictSorted, E> {
                Ok(StrictSorted)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<StrictSorted, A::Error> {
                while a.next_element::<StrictSorted>()?.is_some() {}
                Ok(StrictSorted)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<StrictSorted, A::Error> {
                let mut prev: Option<String> = None;
                while let Some(k) = a.next_key::<String>()? {
                    if prev.as_deref().is_some_and(|p| p >= k.as_str()) {
                        return Err(serde::de::Error::custom(format!(
                            "key {k:?} is not after {prev:?}"
                        )));
                    }
                    prev = Some(k);
                    a.next_value::<StrictSorted>()?;
                }
                Ok(StrictSorted)
            }
        }
        d.deserialize_any(V)
    }
}

fn assert_valid_sorted_json(bytes: &[u8]) {
    let text = std::str::from_utf8(bytes).expect("canonical output is UTF-8");
    serde_json::from_str::<StrictSorted>(text)
        .unwrap_or_else(|e| panic!("not valid sorted unique-key JSON ({e}): {text}"));
    assert!(!text.contains('\n'), "a raw newline in canonical output: {text:?}");
}

// ---------------------------------------------------------------- properties

proptest! {
    #![proptest_config(Config::default())]

    /// Every generated finite value canonicalizes, to valid sorted JSON.
    #[test]
    fn canonical_output_is_valid_sorted_json(v in arb_value()) {
        let bytes = to_canonical_json(&v).expect("a finite value always canonicalizes");
        assert_valid_sorted_json(&bytes);
    }

    /// Insertion order of mapping keys never reaches the output.
    #[test]
    fn canonical_output_ignores_insertion_order(v in arb_value()) {
        prop_assert_eq!(to_canonical_json(&v).unwrap(), to_canonical_json(&reorder(&v)).unwrap());
    }

    /// `digest` is `sha256:` + 64 lowercase hex, deterministic, and moves with the input.
    #[test]
    fn digest_shape_and_sensitivity(a in arb_value(), b in arb_value()) {
        let (ca, cb) = (to_canonical_json(&a).unwrap(), to_canonical_json(&b).unwrap());
        let d = digest(&ca);
        prop_assert!(d.starts_with("sha256:") && d.len() == 7 + 64);
        prop_assert!(d[7..].bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)));
        prop_assert_eq!(&d, &digest(&ca));
        prop_assert_eq!(ca == cb, d == digest(&cb));
    }

    /// A value equals itself under the exact comparator, with a verdict.
    #[test]
    fn exact_compare_is_reflexive(v in arb_value()) {
        prop_assert_eq!(compare(&v, &v, CompareType::Exact), (true, true));
    }

    /// A value and its reordered twin are exactly equal.
    #[test]
    fn exact_compare_ignores_insertion_order(v in arb_value()) {
        prop_assert_eq!(compare(&v, &reorder(&v), CompareType::Exact), (true, true));
    }

    /// Whatever text the BigInt constructor accepts has a canonical form that
    /// is a JSON integer without leading zeros, and the value canonicalizes to it.
    #[test]
    fn accepted_bigint_text_has_a_json_integer_form(s in prop_oneof![3 => "[+-]?[0-9]{0,6}", 1 => any::<String>()]) {
        if let Ok(n) = BigIntValue::try_new(s.clone()) {
            let c = n.canonical();
            let digits = c.strip_prefix('-').unwrap_or(&c);
            prop_assert!(!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()), "{s:?} -> {c:?}");
            prop_assert!(digits == "0" || !digits.starts_with('0'), "{s:?} -> {c:?}");
            prop_assert!(c != "-0", "{s:?}");
            prop_assert_eq!(to_canonical_json(&Value::BigInt(n)).unwrap(), c.into_bytes());
        } else {
            let digits = s.strip_prefix(['+', '-']).unwrap_or(&s);
            prop_assert!(digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()), "{s:?} was refused");
        }
    }

    /// `Map::from_pairs` accepts exactly the key lists without a repeat, in order.
    #[test]
    fn from_pairs_accepts_exactly_unique_keys(keys in prop::collection::vec("[a-d]{0,2}", 0..8)) {
        let pairs: Vec<(String, Value)> = keys.iter().cloned().map(|k| (k, Value::Null)).collect();
        let unique = keys.iter().collect::<std::collections::HashSet<_>>().len() == keys.len();
        match Map::from_pairs(pairs) {
            Ok(m) => {
                prop_assert!(unique);
                prop_assert_eq!(m.keys(), keys.iter().map(String::as_str).collect::<Vec<_>>());
            }
            Err(_) => prop_assert!(!unique),
        }
    }

    /// Arbitrary bytes never panic the reader, and whatever it accepts either
    /// canonicalizes or is refused for a non-finite float -- never a panic.
    #[test]
    fn reader_never_panics_on_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
        if let Ok(v) = reader::parse(&bytes, Stage::Read, "$", "input") {
            let _ = to_canonical_json(&v);
        }
    }

    /// Same, over text that looks like YAML (flow/block syntax, anchors, tags, numbers).
    #[test]
    fn reader_never_panics_on_yaml_shaped_text(text in "[a-z0-9:\\-\\[\\]{},.&*!'\" #~\n]{0,80}") {
        if let Ok(v) = reader::parse(text.as_bytes(), Stage::Read, "$", "input") {
            if let Ok(bytes) = to_canonical_json(&v) {
                assert_valid_sorted_json(&bytes);
            }
        }
    }

    /// The JSON text of a generated value reads back to a value with the same
    /// canonical form (the reader and the canonicalizer agree on JSON).
    #[test]
    fn json_text_round_trips_through_the_reader(v in arb_value()) {
        let doc = wrapped(&v);
        let text = to_json_text(&doc);
        let read = reader::parse(text.as_bytes(), Stage::Read, "$", "input")
            .unwrap_or_else(|e| panic!("reader refused {text}: {e}"));
        prop_assert_eq!(to_canonical_json(&read).unwrap(), to_canonical_json(&doc).unwrap(), "{}", text);
    }
}

// ------------------------------------------------------- cross-language corpus

/// Writes `CIC_PROP_CORPUS_OUT` (JSON lines: `input` = JSON text, `canonical`,
/// `digest`) from a fixed seed, for `go/canonical/crosslang_test.go` to
/// re-derive. No-op unless the variable is set. Expected values come from the
/// Rust pipeline as a user runs it: parse the JSON text, canonicalize.
#[test]
fn export_cross_language_corpus() {
    let Some(path) = std::env::var_os("CIC_PROP_CORPUS_OUT") else {
        return;
    };
    let cases: usize = std::env::var("CIC_PROP_CORPUS_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let mut runner = TestRunner::deterministic();
    let strategy = arb_value();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&path).expect("create corpus"));
    let mut seen = BTreeMap::new();
    for _ in 0..cases {
        let v = wrapped(&strategy.new_tree(&mut runner).expect("generate").current());
        let text = to_json_text(&v);
        let read = reader::parse(text.as_bytes(), Stage::Read, "$", "input").expect("reader");
        let canonical = String::from_utf8(to_canonical_json(&read).unwrap()).unwrap();
        let line = serde_json::json!({
            "input": text,
            "canonical": canonical,
            "digest": digest(canonical.as_bytes()),
        });
        seen.insert(canonical, ());
        writeln!(out, "{line}").unwrap();
    }
    out.flush().unwrap();
    assert!(seen.len() > cases / 4, "generator produced too few distinct cases");
}
