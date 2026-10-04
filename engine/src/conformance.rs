//! Intent↔observed comparison — section F (`docs/MATERIALIZATION-SPEC.md`),
//! ported from `CIC-Relay`'s `core/nexus/iac/compare.go` and
//! `observation.go`'s `ClassifyField`/`ClassifyFieldValue`.
//!
//! # Scope — the comparator primitive, not the object-level walker
//!
//! This is F1's named gap closing one layer, not all of it: [`compare`],
//! [`classify_field`] and [`classify_field_value`] are the per-field
//! primitives `compare.go`/`observation.go` implement. The object-level
//! walk — `conformance.go`'s `ConformancePlan`/`Evaluate`/`aggregate`,
//! which drives these primitives over a whole document and produces the
//! `ConformanceResult` verdict F5's `PlanDigestProjection`/
//! `ObservationDigestProjection` describe — is not implemented here. F1's
//! gap is narrower than it was, not closed.
//!
//! # Coverage: B3's four values, not Relay's two-list envelope
//!
//! `BOUNDARY.md`/B3 (closed, this document) decided `coverage` is
//! `observed | absent | unobserved | unknown`. Relay's own
//! `Observation{Observed, AuthoritativeAbsent}` is a two-list envelope with
//! no third/fourth state at all — `unknown` is B2's own addition, with no
//! Relay equivalent and no landed classification precedent. [`Coverage`]
//! uses the four-value enum directly; there is no `Observation` struct here
//! to adapt, because the object-level walker that would hold one is not
//! built yet.

use crate::canonical::{canonical_float, canonical_integer, to_canonical_json};
use crate::reader::Value;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::FromPrimitive;
use std::str::FromStr;

/// B3's coverage axis. See module docs for why this is four values, not
/// Relay's two-list envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    Observed,
    Absent,
    Unobserved,
    Unknown,
}

impl Coverage {
    /// The exact string F5's `ObservationDigestProjection` commits to per
    /// path — `observed`/`absent`/`unobserved`/`unknown`, verbatim.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Coverage::Observed => "observed",
            Coverage::Absent => "absent",
            Coverage::Unobserved => "unobserved",
            Coverage::Unknown => "unknown",
        }
    }
}

/// `compare.go`'s `CompareType` — the field's `behavior.compare` annotation.
/// Go's empty/unknown string defaults to `CompareExact` at the point a
/// schema's annotation is parsed into this type; that parsing doesn't exist
/// yet (it belongs to the not-yet-built `ConformancePlan` builder), so
/// there is no "empty" variant to represent here — a future builder that
/// turns an annotation string into this enum should default unparsed/absent
/// input to `Exact`, mirroring Go's `default: // CompareExact` branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareType {
    Exact,
    Numeric,
}

impl CompareType {
    /// The literal string F5's `PlanDigestProjection` commits to per field.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CompareType::Exact => "exact",
            CompareType::Numeric => "numeric",
        }
    }
}

/// Per-field conformance verdict — `observation.go`'s `FieldVerdict`,
/// identical constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldVerdict {
    Conformant,
    Drift,
    ObservedAbsent,
    Unobserved,
    NotComparable,
}

/// Ported from `compare.go`'s `Compare`: whether `observed` conforms to
/// `intent` under `ct`. Returns `(matched, comparable)` — `matched` is
/// meaningful only when `comparable` is `true`; the caller surfaces an
/// incomparable pair as `NOT_COMPARABLE`, never as a false `DRIFT`.
#[must_use]
pub fn compare(intent: &Value, observed: &Value, ct: CompareType) -> (bool, bool) {
    match ct {
        CompareType::Numeric => compare_numeric(intent, observed),
        CompareType::Exact => compare_exact(intent, observed),
    }
}

fn compare_numeric(intent: &Value, observed: &Value) -> (bool, bool) {
    match (as_rational(intent), as_rational(observed)) {
        (Some(a), Some(b)) => (a == b, true),
        _ => (false, false),
    }
}

fn is_numeric_value(v: &Value) -> bool {
    matches!(v, Value::Int(_) | Value::BigInt(_) | Value::Float(_))
}

/// `compare.go`'s `jsonKind`: the JSON kind of a value, for exact
/// type-compatibility. All three numeric `Value` variants collapse to
/// `"number"`, matching Go collapsing every numeric Go type the same way.
fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Int(_) | Value::BigInt(_) | Value::Float(_) => "number",
        Value::Str(_) => "string",
        Value::Seq(_) => "array",
        Value::Map(_) => "object",
    }
}

fn compare_exact(intent: &Value, observed: &Value) -> (bool, bool) {
    let i_num = is_numeric_value(intent);
    let o_num = is_numeric_value(observed);
    if i_num || o_num {
        if i_num != o_num {
            return (false, false); // number vs non-number -> not comparable
        }
        return (
            canonical_number_string(intent) == canonical_number_string(observed),
            true,
        );
    }
    if json_kind(intent) != json_kind(observed) {
        return (false, false);
    }
    (canonical_equal(intent, observed), true)
}

/// Canonical decimal form of a numeric `Value`, reusing section A's own
/// number canonicalization (`canonical.rs`) rather than a second
/// implementation — the same digits `to_canonical_json` would write for
/// this value, without re-serializing the whole tree.
fn canonical_number_string(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::BigInt(s) => {
            canonical_integer(s).expect("BigInt invariant: s is always a validated digit run")
        }
        Value::Float(f) => canonical_float(*f),
        _ => unreachable!("canonical_number_string is only called where is_numeric_value(v)"),
    }
}

/// `compare.go`'s `canonicalEqual`, reusing section A's canonicalizer
/// (`to_canonical_json`) instead of a second serializer. A value that
/// fails to canonicalize (a non-finite float, per A's own rejection) is
/// treated as unequal rather than panicking — it should never reach this
/// function on an already-materialized value, but `compare_exact` must
/// stay total either way.
fn canonical_equal(a: &Value, b: &Value) -> bool {
    matches!((to_canonical_json(a), to_canonical_json(b)), (Ok(x), Ok(y)) if x == y)
}

/// Ported from `compare.go`'s `asRat`: coerces a value to an exact rational
/// for numeric-comparator equality — never a lossy `f64` midpoint. Returns
/// `None` for anything that isn't numeric-compatible, mirroring `asRat`'s
/// own `(nil, false)`, including non-finite floats (`BigRational::from_f64`
/// returns `None` for `NaN`/`Infinity`, matching `big.Rat.SetFloat64`'s
/// documented `nil` for the same inputs, verified empirically against both
/// in this session).
fn as_rational(v: &Value) -> Option<BigRational> {
    match v {
        Value::Int(i) => Some(BigRational::from_integer(BigInt::from(*i))),
        Value::BigInt(s) => BigInt::from_str(s).ok().map(BigRational::from_integer),
        Value::Float(f) => BigRational::from_f64(*f),
        Value::Str(s) => decimal_string_to_rational(s),
        Value::Null | Value::Bool(_) | Value::Seq(_) | Value::Map(_) => None,
    }
}

/// Exact decimal-string → rational, mirroring `big.Rat.SetString`'s decimal
/// grammar (`[sign] digits ['.' digits] [('e'|'E') [sign] digits]`) —
/// **deliberately not** going through `f64`, which would silently reproduce
/// `canonicalNumericString`'s float-rounding path (a *different* Go
/// function, for a different purpose: section A's canonical-form
/// normalization, not numeric-comparator equality).
///
/// Verified empirically against real Go `big.Rat.SetString` output for
/// every case this function's tests exercise, including the one that
/// actually matters: `"0.1"` parses to the *exact* decimal `1/10`, not the
/// binary-rounded value an `f64` route would give (`3602879701896397/
/// 36028797018963968`, confirmed via `num_rational::BigRational::from_f64`
/// in this session) — a real, Go-faithful asymmetry between how a
/// `Value::Str` numeric literal and a `Value::Float` are each converted.
/// This is not a bug to fix here: fixing it would make this comparator
/// diverge *from* the Go reference it must stay differentially comparable
/// against (section G's whole point). If the intent/observed data never
/// actually mixes a decimal-string literal against a binary float for the
/// same logical value, the asymmetry is latent, exactly as it is in Relay
/// today — named so it is a known, inherited property, not a rediscovered
/// surprise later.
fn decimal_string_to_rational(s: &str) -> Option<BigRational> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let neg = match bytes.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start_int = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    let int_part = &s[start_int..i];
    let mut frac_part = "";
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let start_frac = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        frac_part = &s[start_frac..i];
    }
    if int_part.is_empty() && frac_part.is_empty() {
        return None; // no digits anywhere -- not a number
    }
    let mut exp: i64 = 0;
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        let exp_neg = match bytes.get(i) {
            Some(b'-') => {
                i += 1;
                true
            }
            Some(b'+') => {
                i += 1;
                false
            }
            _ => false,
        };
        let start_exp = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start_exp {
            return None; // 'e'/'E' with no exponent digits
        }
        let e: i64 = s[start_exp..i].parse().ok()?;
        exp = if exp_neg { -e } else { e };
    }
    if i != bytes.len() {
        return None; // trailing content that isn't part of the grammar
    }
    let mut mantissa_digits = String::with_capacity(int_part.len() + frac_part.len());
    mantissa_digits.push_str(int_part);
    mantissa_digits.push_str(frac_part);
    if mantissa_digits.is_empty() {
        mantissa_digits.push('0');
    }
    let mantissa = BigInt::from_str(&mantissa_digits).ok()?;
    let mantissa = if neg { -mantissa } else { mantissa };
    let net_exponent = exp - i64::try_from(frac_part.len()).ok()?;
    let ten = BigInt::from(10);
    Some(if net_exponent >= 0 {
        let scale = num_traits::Pow::pow(ten, u32::try_from(net_exponent).ok()?);
        BigRational::from_integer(mantissa * scale)
    } else {
        let scale = num_traits::Pow::pow(ten, u32::try_from(-net_exponent).ok()?);
        BigRational::new(mantissa, scale)
    })
}

/// Ported from `observation.go`'s `ClassifyField`: coverage alone decides
/// the verdict when the value is irrelevant (absent/unobserved/unknown);
/// `matched` (from [`compare`]) decides it only when the field was actually
/// observed.
///
/// `Coverage::Unknown` has no Relay equivalent and no landed classification
/// rule (B2's own addition — see module docs). **A new, narrow decision,
/// not a port:** treated the same as `Unobserved` — a device-reported
/// indeterminate value is not authoritative evidence either way, so
/// conformance cannot be claimed from it. This is the narrowest extension
/// of the existing rule that doesn't invent a richer one; a future decision
/// may give `unknown` its own verdict (e.g. a dedicated `INDETERMINATE`)
/// without this function's other branches needing to change.
#[must_use]
pub fn classify_field(coverage: Coverage, intent_present: bool, matched: bool) -> FieldVerdict {
    match coverage {
        Coverage::Observed => {
            if matched {
                FieldVerdict::Conformant
            } else {
                FieldVerdict::Drift
            }
        }
        Coverage::Absent => {
            if intent_present {
                FieldVerdict::Drift
            } else {
                FieldVerdict::ObservedAbsent
            }
        }
        Coverage::Unobserved | Coverage::Unknown => FieldVerdict::Unobserved,
    }
}

/// Ported from `compare.go`'s `ClassifyFieldValue`: joins coverage with the
/// value comparison. Only `Coverage::Observed` ever reads a value — every
/// other coverage state means the comparison is irrelevant and `compare`
/// is never invoked, matching Go exactly (not even a nil-check on the
/// value happens in that branch). This is the same fact F5's
/// `ObservationDigestProjection` grounds its own `value`-presence rule in.
///
/// A field absent from the intent side is represented by the caller as
/// `&Value::Null`, the same convention Go's `resolvePath` uses (a missing
/// path resolves to a zero `interface{}`, i.e. `nil`) — this function does
/// not special-case "missing" itself, only `intent_present` does, exactly
/// as in Go.
#[must_use]
pub fn classify_field_value(
    coverage: Coverage,
    intent_present: bool,
    intent: &Value,
    observed: &Value,
    ct: CompareType,
) -> FieldVerdict {
    if coverage != Coverage::Observed {
        return classify_field(coverage, intent_present, false);
    }
    let (matched, comparable) = compare(intent, observed, ct);
    if !comparable {
        return FieldVerdict::NotComparable;
    }
    classify_field(coverage, intent_present, matched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Map;

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = Map::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
    }

    // Mirrors TestCompare_Exact in compare_test.go, case for case.
    #[test]
    fn compare_exact_mirrors_go() {
        let cases: &[(&str, Value, Value, bool, bool)] = &[
            (
                "equal strings",
                Value::Str("prod".into()),
                Value::Str("prod".into()),
                true,
                true,
            ),
            (
                "different strings",
                Value::Str("prod".into()),
                Value::Str("dev".into()),
                false,
                true,
            ),
            ("equal ints", Value::Int(16), Value::Int(16), true, true),
            (
                "different ints",
                Value::Int(16),
                Value::Int(32),
                false,
                true,
            ),
            (
                "int vs float same value",
                Value::Int(16),
                Value::Float(16.0),
                true,
                true,
            ),
            (
                "string vs number -> not comparable",
                Value::Str("16".into()),
                Value::Int(16),
                false,
                false,
            ),
            (
                "null vs string -> not comparable",
                Value::Null,
                Value::Str("x".into()),
                false,
                false,
            ),
            ("equal nulls", Value::Null, Value::Null, true, true),
        ];
        for (name, intent, observed, want_matched, want_comparable) in cases {
            let (matched, comparable) = compare(intent, observed, CompareType::Exact);
            assert_eq!(comparable, *want_comparable, "{name}: comparable");
            if *want_comparable {
                assert_eq!(matched, *want_matched, "{name}: matched");
            }
        }
        // "number vs object -> not comparable"
        let (_, comparable) = compare(&Value::Int(16), &map(&[]), CompareType::Exact);
        assert!(!comparable, "number vs object must not be comparable");
        // "equal maps" / "different maps"
        let a = map(&[("a", Value::Str("x".into()))]);
        let b = map(&[("a", Value::Str("x".into()))]);
        let c = map(&[("a", Value::Str("y".into()))]);
        let (matched, comparable) = compare(&a, &b, CompareType::Exact);
        assert!(comparable && matched, "equal maps must match");
        let (matched, comparable) = compare(&a, &c, CompareType::Exact);
        assert!(comparable && !matched, "different maps must not match");
    }

    // Mirrors TestCompare_Numeric in compare_test.go, case for case.
    #[test]
    fn compare_numeric_mirrors_go() {
        let cases: &[(&str, Value, Value, bool, bool)] = &[
            (
                "int vs float same value",
                Value::Int(1),
                Value::Float(1.0),
                true,
                true,
            ),
            (
                "int vs numeric string",
                Value::Int(1),
                Value::Str("1".into()),
                true,
                true,
            ),
            (
                "float vs scientific string",
                Value::Float(100.0),
                Value::Str("1e2".into()),
                true,
                true,
            ),
            (
                "decimal string equality",
                Value::Str("1.5".into()),
                Value::Float(1.5),
                true,
                true,
            ),
            (
                "different numbers",
                Value::Int(1),
                Value::Int(2),
                false,
                true,
            ),
            (
                "non-numeric string -> not comparable",
                Value::Str("abc".into()),
                Value::Int(1),
                false,
                false,
            ),
            (
                "bool is not numeric",
                Value::Bool(true),
                Value::Int(1),
                false,
                false,
            ),
        ];
        for (name, intent, observed, want_matched, want_comparable) in cases {
            let (matched, comparable) = compare(intent, observed, CompareType::Numeric);
            assert_eq!(comparable, *want_comparable, "{name}: comparable");
            if *want_comparable {
                assert_eq!(matched, *want_matched, "{name}: matched");
            }
        }
        let (_, comparable) = compare(&map(&[]), &Value::Int(1), CompareType::Numeric);
        assert!(!comparable, "object -> not comparable under numeric");
    }

    // The precision fact decimal_string_to_rational's own doc comment names:
    // "0.1" parses EXACTLY as 1/10, not the binary-rounded f64 value --
    // verified against real Go big.Rat.SetString("0.1") output in this
    // session (1/10, not 3602879701896397/36028797018963968).
    #[test]
    fn decimal_string_parses_exact_not_binary_rounded() {
        let exact = decimal_string_to_rational("0.1").expect("valid decimal");
        let from_binary_float = BigRational::from_f64(0.1).expect("finite");
        assert_ne!(
            exact, from_binary_float,
            "0.1 the decimal string must NOT equal 0.1 the binary float -- \
             this is the real Go asymmetry, not a bug"
        );
        assert_eq!(*exact.numer(), BigInt::from(1));
        assert_eq!(*exact.denom(), BigInt::from(10));
    }

    // Every vector here was generated by running real Go math/big.Rat.SetString
    // in a Docker container in this session, not transcribed from memory.
    #[test]
    fn decimal_string_to_rational_matches_go_big_rat() {
        let cases: &[(&str, i64, i64)] = &[
            ("1.5", 3, 2),
            ("1e2", 100, 1),
            ("0.1", 1, 10),
            ("16", 16, 1),
            ("16.0", 16, 1),
            ("-0.5", -1, 2),
            ("1e-3", 1, 1000),
        ];
        for (s, num, den) in cases {
            let r = decimal_string_to_rational(s).unwrap_or_else(|| panic!("{s} should parse"));
            assert_eq!(*r.numer(), BigInt::from(*num), "{s}: numerator");
            assert_eq!(*r.denom(), BigInt::from(*den), "{s}: denominator");
        }
        for bad in ["abc", "", "e5", "1.2.3"] {
            assert!(
                decimal_string_to_rational(bad).is_none(),
                "{bad:?} must not parse"
            );
        }
    }

    // Mirrors TestClassifyFieldValue in compare_test.go, case for case --
    // using Coverage directly rather than Relay's Observation{Observed,
    // AuthoritativeAbsent} envelope, since this crate has no object-level
    // walker (and no two-list envelope) yet; see module docs.
    #[test]
    fn classify_field_value_mirrors_go() {
        let flex4 = Value::Str("E4.Flex".into());
        let flex3 = Value::Str("E3.Flex".into());

        // observed + matched -> CONFORMANT
        assert_eq!(
            classify_field_value(Coverage::Observed, true, &flex4, &flex4, CompareType::Exact),
            FieldVerdict::Conformant
        );
        // observed + mismatch -> DRIFT
        assert_eq!(
            classify_field_value(Coverage::Observed, true, &flex4, &flex3, CompareType::Exact),
            FieldVerdict::Drift
        );
        // observed + not comparable -> NOT_COMPARABLE
        assert_eq!(
            classify_field_value(
                Coverage::Observed,
                true,
                &Value::Str("16".into()),
                &Value::Int(16),
                CompareType::Exact
            ),
            FieldVerdict::NotComparable
        );
        // numeric normalization -> CONFORMANT
        assert_eq!(
            classify_field_value(
                Coverage::Observed,
                true,
                &Value::Int(16),
                &Value::Float(16.0),
                CompareType::Numeric
            ),
            FieldVerdict::Conformant
        );
        // unobserved -> UNOBSERVED (value ignored)
        assert_eq!(
            classify_field_value(
                Coverage::Unobserved,
                true,
                &flex4,
                &Value::Str("anything".into()),
                CompareType::Exact
            ),
            FieldVerdict::Unobserved
        );
        // authoritative absent + intent wants it -> DRIFT
        assert_eq!(
            classify_field_value(
                Coverage::Absent,
                true,
                &Value::Null,
                &Value::Null,
                CompareType::Exact
            ),
            FieldVerdict::Drift
        );
        // authoritative absent + intent omits it -> OBSERVED_ABSENT
        assert_eq!(
            classify_field_value(
                Coverage::Absent,
                false,
                &Value::Null,
                &Value::Null,
                CompareType::Exact
            ),
            FieldVerdict::ObservedAbsent
        );
    }

    // No Relay precedent for this -- Coverage::Unknown is B2's own addition.
    // Pinned down here as a decision this crate makes, not inherited.
    #[test]
    fn unknown_coverage_classifies_like_unobserved() {
        assert_eq!(
            classify_field_value(
                Coverage::Unknown,
                true,
                &Value::Str("x".into()),
                &Value::Str("fault".into()),
                CompareType::Exact
            ),
            FieldVerdict::Unobserved
        );
    }

    #[test]
    fn coverage_and_compare_type_strings_match_f5_projection_literals() {
        assert_eq!(Coverage::Observed.as_str(), "observed");
        assert_eq!(Coverage::Absent.as_str(), "absent");
        assert_eq!(Coverage::Unobserved.as_str(), "unobserved");
        assert_eq!(Coverage::Unknown.as_str(), "unknown");
        assert_eq!(CompareType::Exact.as_str(), "exact");
        assert_eq!(CompareType::Numeric.as_str(), "numeric");
    }
}
