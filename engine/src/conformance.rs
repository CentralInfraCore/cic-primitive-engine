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
//!
//! # The numeric comparator's string grammar matches `SetString` in full
//!
//! [`rat_from_string`] ports `big.Rat.SetString`'s complete grammar, not a
//! plausible-looking decimal/scientific subset of it — decimal, binary,
//! octal and hex integers and floats, the `"a/b"` fraction form with either
//! side independently based, `"e"`/`"p"` exponents, and digit-separating
//! underscores. Closed across two review rounds on PR #25, each one
//! verified against real Go output before being encoded — see that
//! function's own doc comment for what each round found and fixed.

use crate::canonical::{canonical_float, canonical_integer, to_canonical_json};
use crate::reader::Value;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{FromPrimitive, Num};
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

/// Per-path coverage lookup for the not-yet-built object-level walker
/// (`plan.rs`'s `evaluate`) — **not** a port of `observation.go`'s
/// `Observation{Observed, AuthoritativeAbsent}` two-list envelope. There is
/// no Go struct to adapt: B3's four-value [`Coverage`] enum is the model
/// this engine already committed to (see this module's own doc comment),
/// so the lookup is keyed on it directly rather than reconstructed from two
/// path lists. A path with no explicit entry defaults to
/// `Coverage::Unobserved`, mirroring `Observation.Coverage`'s own default
/// case (`observation.go`: *"a path in neither set is unobserved"*).
#[derive(Debug, Clone, Default)]
pub struct Observation(std::collections::HashMap<String, Coverage>);

impl Observation {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `path`'s coverage. A second call for the same path replaces
    /// the first -- this type makes no claim about which call "should win"
    /// for a path recorded twice; that is the caller's own invariant to
    /// hold, same as it would be in Go.
    pub fn set(&mut self, path: impl Into<String>, coverage: Coverage) {
        self.0.insert(path.into(), coverage);
    }

    #[must_use]
    pub fn coverage(&self, path: &str) -> Coverage {
        self.0.get(path).copied().unwrap_or(Coverage::Unobserved)
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
        Value::Str(s) => rat_from_string(s),
        Value::Null | Value::Bool(_) | Value::Seq(_) | Value::Map(_) => None,
    }
}

/// Ported from `compare.go`'s `ratFromString`, which is a thin wrapper over
/// `big.Rat.SetString` with no narrowing of its own — so this function's
/// grammar must match `SetString`'s *full* grammar, not a plausible-looking
/// subset of it (Go's own doc for `SetString`, `go doc math/big.Rat.
/// SetString`, is the normative source; every claim below was additionally
/// verified empirically in a Go container, not taken from the doc text
/// alone).
///
/// **Correction (review-caught on PR #25, in two rounds): an earlier
/// version implemented only the decimal/scientific form.** Round one added
/// the signed fraction form `"a/b"` (Go: *"s can be given as a (possibly
/// signed) fraction `a/b`, or as a floating-point number..."*) after review
/// showed `compare(Str("1/2"), Str("0.5"), Numeric)` disagreed with Go.
/// Round two closed the rest: `SetString` also lets either side of a
/// fraction, or a float's mantissa, carry a `"0b"`/`"0o"`/`"0x"` prefix for
/// a binary/octal/hexadecimal integer; a float's exponent is `"e"`/`"E"`
/// (×10, any non-hex base — hex can't use `e`, since `e` is itself a valid
/// hex digit) or `"p"`/`"P"` (×2, *any* base, including decimal — verified:
/// `"1p1"` → `2`); and a single underscore may separate two digits of any
/// digit run, plus — uniquely — immediately after a base prefix and before
/// its first digit (`"0x_10"` → `16`, but `"0x__10"`/`"0x_"`/`"_0x10"` all
/// fail). Every one of these, and the places they fail, was checked against
/// real Go output before being encoded below (this module's own tests
/// carry the vectors). This closes the grammar-parity gap the first
/// round's doc comment named rather than closed.
fn rat_from_string(s: &str) -> Option<BigRational> {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let neg = parse_sign(bytes, &mut i);
    let after_sign = i;

    // Fraction and float/int forms never overlap (a fraction always has a
    // top-level '/', a float/int literal never does) -- mirrors
    // SetString's own either/or structure, tried in the same order.
    let mut j = after_sign;
    if let Some(r) = parse_fraction(bytes, &mut j) {
        if j == bytes.len() {
            return Some(if neg { -r } else { r });
        }
    }

    i = after_sign;
    let v = parse_float_or_int(bytes, &mut i)?;
    if i != bytes.len() {
        return None; // trailing content outside the grammar
    }
    Some(if neg { -v } else { v })
}

/// One of `SetString`'s four mantissa/integer radixes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Radix {
    Dec,
    Bin,
    Oct,
    Hex,
}

impl Radix {
    fn value(self) -> u32 {
        match self {
            Radix::Dec => 10,
            Radix::Bin => 2,
            Radix::Oct => 8,
            Radix::Hex => 16,
        }
    }

    fn is_digit(self, b: u8) -> bool {
        match self {
            Radix::Dec => b.is_ascii_digit(),
            Radix::Bin => b == b'0' || b == b'1',
            Radix::Oct => (b'0'..=b'7').contains(&b),
            Radix::Hex => b.is_ascii_hexdigit(),
        }
    }
}

fn parse_sign(bytes: &[u8], i: &mut usize) -> bool {
    match bytes.get(*i) {
        Some(b'-') => {
            *i += 1;
            true
        }
        Some(b'+') => {
            *i += 1;
            false
        }
        _ => false,
    }
}

/// `"0b"`/`"0B"`/`"0o"`/`"0O"`/`"0x"`/`"0X"`, consumed if present; `Dec`
/// (consuming nothing) otherwise. A bare leading `"0"` is decimal, per
/// `SetString`'s own doc: *"a leading 0 is considered a decimal leading 0;
/// it does not indicate octal representation"* (verified: `"0123"` → `123`,
/// not rejected and not treated as legacy octal).
fn detect_prefix(bytes: &[u8], i: &mut usize) -> Radix {
    if bytes.get(*i) == Some(&b'0') {
        match bytes.get(*i + 1) {
            Some(b'b' | b'B') => {
                *i += 2;
                return Radix::Bin;
            }
            Some(b'o' | b'O') => {
                *i += 2;
                return Radix::Oct;
            }
            Some(b'x' | b'X') => {
                *i += 2;
                return Radix::Hex;
            }
            _ => {}
        }
    }
    Radix::Dec
}

/// Scans the longest valid run of `radix`-digits at `*i`, allowing a single
/// underscore between any two digits, plus — only when `allow_leading` is
/// set — a single underscore immediately at the start, before the first
/// digit (the base-prefix exception). Returns the digits with underscores
/// stripped; an empty result means "no digits here," not an error — the
/// caller's own "at least one digit" and "whole string consumed" checks do
/// the actual rejecting, the same structure `SetString` itself has (a
/// misplaced underscore simply leaves unconsumed input behind).
fn scan_digits(bytes: &[u8], i: &mut usize, radix: Radix, allow_leading: bool) -> String {
    let mut out = String::new();
    if allow_leading
        && bytes.get(*i) == Some(&b'_')
        && bytes.get(*i + 1).is_some_and(|b| radix.is_digit(*b))
    {
        *i += 1;
    }
    let mut last_was_digit = false;
    loop {
        match bytes.get(*i) {
            Some(&b) if radix.is_digit(b) => {
                out.push(b as char);
                *i += 1;
                last_was_digit = true;
            }
            Some(&b'_')
                if last_was_digit && bytes.get(*i + 1).is_some_and(|b| radix.is_digit(*b)) =>
            {
                *i += 1;
                last_was_digit = false;
            }
            _ => break,
        }
    }
    out
}

/// The `"a/b"` half of `SetString`'s grammar: each side is an independently
/// based integer (`"0x10/2"` → `8`, `"2/0x10"` → `1/8`), the divisor may
/// not be signed and may not be zero (`"1/0"`, `"3/-4"` both fail); the
/// dividend may be zero (`"0/5"`/`"-0/5"` both → `0`). Returns `None`
/// (leaving `*i` unspecified) if there is no `/` at the top level, or
/// either side fails to parse as a plain based integer — the caller must
/// not assume `*i` advanced on `None`.
fn parse_fraction(bytes: &[u8], i: &mut usize) -> Option<BigRational> {
    let start = *i;
    // Parse (or fail to parse) the dividend first, purely to advance `*i`
    // past it, so the '/' check below looks right after whatever digits
    // (if any) it found — the actual numerator value is re-extracted via
    // the `?` below only once a '/' confirms this is a fraction at all.
    let numerator = parse_unsigned_integer_any_base(bytes, i);
    if bytes.get(*i) != Some(&b'/') {
        *i = start; // not a fraction -- let the caller retry as float/int
        return None;
    }
    let numerator = numerator?;
    *i += 1;
    let denominator = parse_unsigned_integer_any_base(bytes, i)?;
    if denominator == BigInt::from(0) {
        return None;
    }
    Some(BigRational::new(numerator, denominator))
}

fn parse_unsigned_integer_any_base(bytes: &[u8], i: &mut usize) -> Option<BigInt> {
    let before = *i;
    let radix = detect_prefix(bytes, i);
    let had_prefix = *i != before;
    let digits = scan_digits(bytes, i, radix, had_prefix);
    if digits.is_empty() {
        return None;
    }
    BigInt::from_str_radix(&digits, radix.value()).ok()
}

/// The floating-point/plain-integer half of `SetString`'s grammar: an
/// optional base prefix, an integer-part digit run, an optional `'.'` and
/// fractional-part digit run (at least one digit somewhere across the
/// two), and an optional exponent — `"e"`/`"E"` (×10, forbidden for `Hex`,
/// where `e` is a mantissa digit) or `"p"`/`"P"` (×2, any radix, including
/// `Dec` — `"1p1"` → `2`). **Deliberately not** going through `f64`
/// anywhere in this path, which would silently reproduce
/// `canonicalNumericString`'s float-rounding path (a *different* Go
/// function, for a different purpose: section A's canonical-form
/// normalization, not numeric-comparator equality) — see this module's own
/// doc comment on `"0.1"` (string) vs `0.1` (`f64`) for why that distinction
/// is load-bearing, not pedantic.
fn parse_float_or_int(bytes: &[u8], i: &mut usize) -> Option<BigRational> {
    let before = *i;
    let radix = detect_prefix(bytes, i);
    let had_prefix = *i != before;
    let int_digits = scan_digits(bytes, i, radix, had_prefix);
    let mut frac_digits = String::new();
    if bytes.get(*i) == Some(&b'.') {
        *i += 1;
        frac_digits = scan_digits(bytes, i, radix, false);
    }
    if int_digits.is_empty() && frac_digits.is_empty() {
        return None;
    }
    let mut mantissa_digits = String::with_capacity(int_digits.len() + frac_digits.len());
    mantissa_digits.push_str(&int_digits);
    mantissa_digits.push_str(&frac_digits);
    if mantissa_digits.is_empty() {
        mantissa_digits.push('0');
    }
    let mantissa = BigInt::from_str_radix(&mantissa_digits, radix.value()).ok()?;
    let frac_len = u32::try_from(frac_digits.len()).ok()?;
    let mut value = BigRational::new(
        mantissa,
        num_traits::Pow::pow(BigInt::from(radix.value()), frac_len),
    );

    match bytes.get(*i) {
        Some(&b'e' | &b'E') if radix != Radix::Hex => {
            *i += 1;
            value = apply_exponent(bytes, i, value, 10)?;
        }
        Some(&b'p' | &b'P') => {
            *i += 1;
            value = apply_exponent(bytes, i, value, 2)?;
        }
        _ => {}
    }
    Some(value)
}

fn apply_exponent(
    bytes: &[u8],
    i: &mut usize,
    value: BigRational,
    exponent_base: u32,
) -> Option<BigRational> {
    let neg = parse_sign(bytes, i);
    let digits = scan_digits(bytes, i, Radix::Dec, false);
    if digits.is_empty() {
        return None; // the marker consumed but no exponent digits -- invalid
    }
    let exp: u32 = digits.parse().ok()?;
    let scale = BigRational::from_integer(num_traits::Pow::pow(BigInt::from(exponent_base), exp));
    Some(if neg { value / scale } else { value * scale })
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

    // The precision fact this module's own doc comment names: "0.1" parses
    // EXACTLY as 1/10, not the binary-rounded f64 value -- verified against
    // real Go big.Rat.SetString("0.1") output in this session (1/10, not
    // 3602879701896397/36028797018963968).
    #[test]
    fn decimal_string_parses_exact_not_binary_rounded() {
        let exact = rat_from_string("0.1").expect("valid decimal");
        let from_binary_float = BigRational::from_f64(0.1).expect("finite");
        assert_ne!(
            exact, from_binary_float,
            "0.1 the decimal string must NOT equal 0.1 the binary float -- \
             this is the real Go asymmetry, not a bug"
        );
        assert_eq!(*exact.numer(), BigInt::from(1));
        assert_eq!(*exact.denom(), BigInt::from(10));
    }

    // Every vector in this and the next test was generated by running real
    // Go math/big.Rat.SetString in a Docker container in this session, not
    // transcribed from memory or from the Go doc comment's prose alone.
    #[test]
    fn decimal_and_fraction_forms_match_go_big_rat() {
        let cases: &[(&str, i64, i64)] = &[
            ("1.5", 3, 2),
            ("1e2", 100, 1),
            ("0.1", 1, 10),
            ("16", 16, 1),
            ("16.0", 16, 1),
            ("-0.5", -1, 2),
            ("1e-3", 1, 1000),
            (".5", 1, 2),
            ("-.5", -1, 2),
            ("5.", 5, 1),
            ("+5", 5, 1),
            // Review-caught gap on PR #25, round one: the fraction half of
            // SetString's grammar ("a/b") was missing entirely.
            ("1/2", 1, 2),
            ("-1/2", -1, 2),
            ("+1/2", 1, 2),
            ("10/5", 2, 1), // auto-reduced, matching Go
            ("0/5", 0, 1),
            ("-0/5", 0, 1),
        ];
        for (s, num, den) in cases {
            let r = rat_from_string(s).unwrap_or_else(|| panic!("{s} should parse"));
            assert_eq!(*r.numer(), BigInt::from(*num), "{s}: numerator");
            assert_eq!(*r.denom(), BigInt::from(*den), "{s}: denominator");
        }
        // "the divisor may not be signed" (Go's own SetString doc) -- and a
        // zero divisor, a decimal numerator/denominator in a fraction, and
        // a doubled separator all fail in Go too.
        for bad in [
            "abc", "", "e5", "1.2.3", "1/0", "3/-4", "1.5/2", "1/2.5", "1//2",
        ] {
            assert!(rat_from_string(bad).is_none(), "{bad:?} must not parse");
        }
    }

    // Review-caught gap on PR #25, round two: SetString's full grammar also
    // covers binary/octal/hex integers and floats (with their own "e"/"p"
    // exponents) and digit-separating underscores -- not ported in round
    // one, which closed only the decimal and plain fraction forms. Every
    // vector here, including every rejection, was verified against real Go
    // output in a Docker container before being encoded.
    #[test]
    fn based_integers_floats_and_underscores_match_go_big_rat() {
        let good: &[(&str, i64, i64)] = &[
            ("0b101", 5, 1),
            ("0B101", 5, 1),
            ("0o17", 15, 1),
            ("0O17", 15, 1),
            ("0x1A", 26, 1),
            ("0X1a", 26, 1),
            ("0x10", 16, 1),
            ("0x10/2", 8, 1),
            ("0b11/0x3", 1, 1),
            ("2/0x10", 1, 8),
            ("1_000", 1000, 1),
            ("0x1_0", 16, 1),
            ("1_0.5", 21, 2),
            ("0b1_0", 2, 1),
            ("0_0", 0, 1),
            ("0x1p0", 1, 1),
            ("0x1.8p1", 3, 1),
            ("0x.8p0", 1, 2),
            ("0x1.8", 3, 2),
            ("0x1e5", 485, 1), // 'e' is a hex DIGIT here, not an exponent
            ("0x1p-1", 1, 2),
            ("0x0p0", 0, 1),
            ("0123", 123, 1), // leading 0 is decimal, never legacy octal
            ("00", 0, 1),
            ("007", 7, 1),
            ("-0x10", -16, 1),
            ("0x1_A", 26, 1),
            ("1_0e10", 100_000_000_000, 1),
            ("0b1.1", 3, 2),
            ("0o1.1", 9, 8),
            ("0b1e1", 10, 1), // 'e' means x10 here -- unambiguous outside hex
            ("0o1e1", 10, 1),
            ("0x_10", 16, 1), // underscore immediately after a base prefix
            ("0b_101", 5, 1),
            ("0o_17", 15, 1),
            ("1.2_3", 123, 100),
            ("0x1P0", 1, 1),
            ("0X1p0", 1, 1),
            ("1p1", 2, 1), // 'p' (x2) works on a plain decimal mantissa too
            ("1p-1", 1, 2),
            ("1P1", 2, 1),
            ("0b1p1", 2, 1),
            ("0o1p1", 2, 1),
            ("0b10.1p1", 5, 1),
            ("0o10.1p1", 65, 4),
            ("0x.1", 1, 16),
            ("0b.1", 1, 2),
            ("0o.1", 1, 8),
        ];
        for (s, num, den) in good {
            let r = rat_from_string(s).unwrap_or_else(|| panic!("{s} should parse"));
            assert_eq!(*r.numer(), BigInt::from(*num), "{s}: numerator");
            assert_eq!(*r.denom(), BigInt::from(*den), "{s}: denominator");
        }
        for bad in [
            "1/0", "3/-4", "0x-10", "_1", "1_", "1__0", "1_.5", "1._5", "0x__10", "0x_", "_0x10",
            "1.2_", "1._2", "0x1._8", "0x1_.8", "0x.8_", "0x_.8", "0x1p_1", "0x1p1_", "0x1p_",
            "1e_1", "1e1_", "+", "-", "/", "1/", "/1", "0x", "0b", "0o", "1e1p1", "10e", "10e+",
            "10p", ".", "0x.", "0b.", "0o.",
        ] {
            assert!(rat_from_string(bad).is_none(), "{bad:?} must not parse");
        }
    }

    // The exact scenario the review named: under the review's own claim,
    // Go's Compare("1/2", "0.5", CompareNumeric) is comparable AND matched.
    // This pins that down as a real test, not just a doc assertion.
    #[test]
    fn numeric_comparator_treats_fraction_string_as_its_value() {
        let (matched, comparable) = compare(
            &Value::Str("1/2".into()),
            &Value::Str("0.5".into()),
            CompareType::Numeric,
        );
        assert!(
            comparable && matched,
            "\"1/2\" must numeric-compare equal to \"0.5\", matching Go"
        );
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
