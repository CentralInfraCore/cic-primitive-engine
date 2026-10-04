//! Canonical byte representation — section A of `docs/MATERIALIZATION-SPEC.md`.
//!
//! One byte representation of a [`Value`] tree, for digests and caches.
//! Mirrors `core/nexus/iac/canonicaljson.go`/`digest.go` (`CIC-Relay`) byte
//! for byte — verified empirically against Go's `encoding/json` in this
//! session, not assumed from documentation. Number canonicalization (A3)
//! already has a proven Rust peer in the `cic-canonical` crate (both its
//! `canonical_float` and `canonical_integer`); the logic is ported here
//! rather than taken as a dependency, so this crate's canonicalization
//! stays self-contained and this engine does not acquire a cross-repository
//! build dependency on `CIC-Relay`.
//!
//! # What this does not cover yet
//!
//! Two named, inherited gaps, neither fixed here:
//! - **Unicode normalization (A4).** Two Unicode-canonically-equivalent
//!   strings (precomposed vs. decomposed) digest to different bytes, exactly
//!   as the existing Go pipeline already does.
//! - **`TopologySet` element canonical order (A6).** Not resolved by the
//!   existing Go implementation either; out of scope until a set-topology
//!   collection actually needs one.
//!
//! B7 (`docs/MATERIALIZATION-SPEC.md`) already decided that applicable
//! `capability`/`coverage`/`provenance` facts must be part of the
//! materialized semantic tree `output_digest` commits to — not left open
//! here. This module only canonicalizes whatever [`Value`] tree it
//! receives; whether that tree's own shape carries those facts is a
//! question for whatever builds the tree (`Normalize`/`Resolve`, not yet
//! implemented), not for this byte-writer.

use crate::error::{code, Error, Result, Stage};
use crate::reader::{Map, Value};
use sha2::{Digest as _, Sha256};

/// Canonical JSON bytes for `v` (A1–A6): compact, no inter-token whitespace,
/// object keys sorted by raw UTF-8 byte order, numbers per A3, strings
/// escaped per A4, arrays kept in source order.
///
/// # Errors
/// Returns `E_NON_FINITE_NUMBER` if any `Value::Float` in the tree is `NaN`
/// or infinite — canonical JSON has no representation for either, and this
/// is reachable from authored YAML's own `.nan`/`.inf` tags, not only from
/// constructing a [`Value`] directly.
pub fn to_canonical_json(v: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_value(&mut out, v, "$")?;
    Ok(out)
}

/// `sha256:` + lowercase-hex(SHA-256(bytes)) — A7. No transform beyond the
/// hash itself; `bytes` is expected to already be canonical (normally the
/// output of [`to_canonical_json`]).
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    let sum = Sha256::digest(bytes);
    let mut out = String::with_capacity(7 + sum.len() * 2);
    out.push_str("sha256:");
    for byte in sum {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn write_value(out: &mut Vec<u8>, v: &Value, path: &str) -> Result<()> {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        // Exact-digit rendering, no precision loss -- `i64` already
        // guarantees this by construction (no leading zeros, no redundant
        // sign, no -0 to fold).
        Value::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        // A3's big.Int-equivalent case (review-caught on PR #19: `reader.rs`
        // used to silently lose this precision to saphyr's own i64/f64
        // fallback -- see `Value::BigInt`'s own doc comment). `reader.rs`
        // only guarantees the text is a validated digit run; normalization
        // -- sign fold, leading-zero strip, `-0` -> `0` -- happens here,
        // ported from `cic-canonical`'s own already-proven
        // `canonical_integer`, the same function Go's `number.go` and this
        // crate's number canonicalization both already trust.
        Value::BigInt(s) => out.extend_from_slice(
            canonical_integer(s)
                .expect("reader.rs only constructs BigInt from a validated digit run")
                .as_bytes(),
        ),
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(Error::new(
                    code::NON_FINITE_NUMBER,
                    "R-CANON-FINITE",
                    Stage::Canonicalize,
                    path,
                    format!(
                        "{path} is {f}, not a finite number; canonical JSON has no \
                         representation for NaN or infinity"
                    ),
                ));
            }
            out.extend_from_slice(canonical_float(*f).as_bytes());
        }
        Value::Str(s) => write_json_string(out, s),
        Value::Seq(items) => write_array(out, items, path)?,
        Value::Map(m) => write_map(out, m, path)?,
    }
    Ok(())
}

/// A3, ported from `cic-canonical`'s own `canonical_integer` (not imported
/// as a dependency — see module docs): sign folded, leading zeros stripped,
/// `-0`/`+0` -> `0` — the same normalization Go's `number.go` applies via
/// `big.Int.SetString(s, 10).String()`, verified empirically against it in
/// this session (`"000000123"` -> `"123"`, `"-00000123"` -> `"-123"`,
/// `"-0"`/`"+0"` -> `"0"`). Returns `None` only if `s` is not actually a
/// validated digit run — [`crate::reader::Value::BigInt`]'s own invariant
/// guarantees this never happens for a value this crate constructed itself.
pub(crate) fn canonical_integer(s: &str) -> Option<String> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let magnitude = digits.trim_start_matches('0');
    Some(if magnitude.is_empty() {
        "0".to_string() // all zeros, including "-0"/"+0"
    } else if neg {
        format!("-{magnitude}")
    } else {
        magnitude.to_string()
    })
}

/// A3, ported from `cic-canonical`'s own `canonical_float` (not imported as a
/// dependency — see module docs): shortest round-trip plain-decimal form,
/// `-0.0` folded to `0`. Rust's `f64` `Display` is shortest round-trip and
/// never uses an exponent, matching Go's `FormatFloat(f, 'f', -1, 64)` —
/// verified empirically in this session (`4.0` -> `"4"`, `1000.0` ->
/// `"1000"`, not `"1e3"`).
pub(crate) fn canonical_float(f: f64) -> String {
    let s = format!("{f}");
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

fn write_array(out: &mut Vec<u8>, items: &[Value], path: &str) -> Result<()> {
    out.push(b'[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        write_value(out, item, &format!("{path}[{i}]"))?;
    }
    out.push(b']');
    Ok(())
}

fn write_map(out: &mut Vec<u8>, m: &Map, path: &str) -> Result<()> {
    out.push(b'{');
    // Raw UTF-8 byte order -- `str`'s `Ord` is already byte-wise comparison,
    // matching Go's `sort.Strings` (A2) with no custom comparator needed.
    let mut keys = m.keys();
    keys.sort_unstable();
    for (i, k) in keys.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        write_json_string(out, k);
        out.push(b':');
        write_value(
            out,
            m.get(k)
                .expect("key came from this same map's own key list"),
            &format!("{path}.{k}"),
        )?;
    }
    out.push(b'}');
    Ok(())
}

/// A4: JSON string escaping exactly matching Go's default `encoding/json`
/// behaviour — verified empirically against it in this session, not
/// against RFC 8259's minimums. Short escapes for backspace/tab/newline/
/// form-feed/carriage-return, the quote and the backslash itself; a
/// six-character lowercase-hex escape for every other control character
/// below U+0020; and the same six-character escape form for four more
/// characters Go escapes that RFC 8259 does not require escaped: the
/// less-than and greater-than signs, the ampersand, and the line/
/// paragraph separators. Inherited from calling the stdlib marshaler
/// directly rather than a deliberate design choice, adopted as-is rather
/// than "fixed" — changing it would silently change every existing
/// digest. Everything else, including multi-byte UTF-8, is written
/// literally; no Unicode normalization is applied (see module docs).
fn write_json_string(out: &mut Vec<u8>, s: &str) {
    out.push(b'"');
    for ch in s.chars() {
        match ch {
            '"' => out.extend_from_slice(b"\\\""),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\u{08}' => out.extend_from_slice(b"\\b"),
            '\u{09}' => out.extend_from_slice(b"\\t"),
            '\u{0A}' => out.extend_from_slice(b"\\n"),
            '\u{0C}' => out.extend_from_slice(b"\\f"),
            '\u{0D}' => out.extend_from_slice(b"\\r"),
            '\u{00}'..='\u{1F}' => {
                out.extend_from_slice(format!("\\u{:04x}", ch as u32).as_bytes());
            }
            '<' => out.extend_from_slice(b"\\u003c"),
            '>' => out.extend_from_slice(b"\\u003e"),
            '&' => out.extend_from_slice(b"\\u0026"),
            '\u{2028}' => out.extend_from_slice(b"\\u2028"),
            '\u{2029}' => out.extend_from_slice(b"\\u2029"),
            _ => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    out.push(b'"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::Map;

    fn json(v: &Value) -> String {
        String::from_utf8(to_canonical_json(v).expect("value is finite"))
            .expect("canonical output is always valid UTF-8")
    }

    fn map(pairs: &[(&str, Value)]) -> Value {
        let mut m = Map::default();
        for (k, v) in pairs {
            m.push(*k, v.clone());
        }
        Value::Map(m)
    }

    // Mirrors TestToJSON_DataTypes in pkg/canonicaljson/canonicaljson_test.go.
    #[test]
    fn data_types_mirror_go() {
        assert_eq!(json(&Value::Null), "null");
        assert_eq!(json(&Value::Bool(true)), "true");
        assert_eq!(json(&Value::Bool(false)), "false");
        assert_eq!(json(&Value::Float(123.456)), "123.456");
        assert_eq!(json(&Value::Float(0.0)), "0");
        assert_eq!(json(&Value::Str("hello world".into())), "\"hello world\"");
        assert_eq!(json(&Value::Seq(vec![])), "[]");
        assert_eq!(
            json(&Value::Seq(vec![
                Value::Float(1.0),
                Value::Str("two".into()),
                Value::Bool(true),
                Value::Null,
            ])),
            "[1,\"two\",true,null]"
        );
        assert_eq!(json(&map(&[])), "{}");
        assert_eq!(
            json(&map(&[
                ("c", Value::Float(3.0)),
                ("a", Value::Float(1.0)),
                ("b", Value::Float(2.0)),
            ])),
            "{\"a\":1,\"b\":2,\"c\":3}"
        );
        assert_eq!(json(&Value::Int(42)), "42");
    }

    // Mirrors the "nested structure" case in the same Go test.
    #[test]
    fn nested_structure_mirrors_go() {
        let nested = map(&[
            ("zulu", Value::Str("last".into())),
            (
                "alpha",
                Value::Seq(vec![
                    Value::Str("one".into()),
                    map(&[("gamma", Value::Bool(true)), ("beta", Value::Bool(false))]),
                ]),
            ),
            ("x-ray", Value::Int(123)),
        ]);
        assert_eq!(
            json(&nested),
            "{\"alpha\":[\"one\",{\"beta\":false,\"gamma\":true}],\"x-ray\":123,\"zulu\":\"last\"}"
        );
    }

    // A3, mirrors TestCanonicalNumber in number_test.go via cic-canonical's
    // own already-proven vectors -- re-verified here against the Value tree,
    // not re-trusting the port without a check.
    #[test]
    fn numbers_mirror_cic_canonical() {
        assert_eq!(json(&Value::Float(4.0)), "4");
        assert_eq!(json(&Value::Float(4.5)), "4.5");
        assert_eq!(json(&Value::Float(-0.0)), "0");
        assert_eq!(json(&Value::Float(1000.0)), "1000");
        assert_eq!(json(&Value::Int(-7)), "-7");
        assert_eq!(json(&Value::Int(0)), "0");
        // Beyond f64's 53-bit mantissa: kept exact, i64 native precision.
        assert_eq!(json(&Value::Int(9_007_199_254_740_993)), "9007199254740993");
        assert_eq!(json(&Value::Int(9_007_199_254_740_992)), "9007199254740992");
    }

    // Review-caught gap (PR #19): the earlier vectors above only covered
    // small magnitudes, which doesn't prove A3's "never scientific
    // notation" claim at the extremes where Rust's f64 Display and Go's
    // FormatFloat could plausibly diverge. Diffed directly against real Go
    // output in this session (a Go container, strconv.FormatFloat(v, 'f',
    // -1, 64)), not assumed from the small-magnitude vectors generalizing.
    #[test]
    fn extreme_magnitude_floats_match_go_format_float_exactly() {
        let vectors: &[(f64, &str)] = &[
            (1e20, "100000000000000000000"),
            (1e21, "1000000000000000000000"),
            (1e30, "1000000000000000000000000000000"),
            (1e-10, "0.0000000001"),
            (1e-100, "0.0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001"),
            // Go's math.SmallestNonzeroFloat64 -- the smallest positive
            // subnormal double, not Rust's f64::MIN_POSITIVE (which is the
            // smallest positive *normal* double, a different value).
            (5e-324, "0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005"),
        ];
        for (v, want) in vectors {
            assert_eq!(json(&Value::Float(*v)), *want, "value {v:e}");
        }
    }

    // Review-caught gap (PR #19): a value this engine could actually see
    // (authored YAML's own `.nan`/`.inf`/`-.inf` core-schema tags -- a real,
    // reachable path, verified empirically against saphyr, not a
    // hypothetical API misuse) must not silently become an invalid JSON
    // token (`NaN`, `inf`) in what claims to be canonical JSON output.
    #[test]
    fn non_finite_floats_are_rejected_not_written() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = to_canonical_json(&Value::Float(v)).expect_err("non-finite must be rejected");
            assert_eq!(err.code, code::NON_FINITE_NUMBER);
            assert_eq!(err.stage, Stage::Canonicalize);
        }
        // The rejection carries the path to the offending value, not just
        // "somewhere in the tree".
        let nested = map(&[("a", Value::Seq(vec![Value::Int(1), Value::Float(f64::NAN)]))]);
        let err = to_canonical_json(&nested).expect_err("nested non-finite must be rejected");
        assert_eq!(err.path, "$.a[1]");
    }

    // A4, verified empirically against Go's encoding/json in this session
    // (MATERIALIZATION-SPEC.md's A4 table) -- re-verified here byte for
    // byte, not re-trusting the table without a check.
    #[test]
    fn string_escaping_matches_go_json_marshal() {
        assert_eq!(
            json(&Value::Str("a<b>c&d".into())),
            "\"a\\u003cb\\u003ec\\u0026d\""
        );
        assert_eq!(
            json(&Value::Str("line\u{2028}sep\u{2029}para".into())),
            "\"line\\u2028sep\\u2029para\""
        );
        // Precomposed "café" (U+00E9) -- different UTF-8 bytes from the
        // decomposed form below, by design (A4's named Unicode-normalization
        // gap): both are legal, neither is canonicalized toward the other.
        assert_eq!(json(&Value::Str("café".into())), "\"café\"");
        assert_eq!(
            to_canonical_json(&Value::Str("café".into())).unwrap(),
            b"\"caf\xc3\xa9\""
        );
        // "cafe" + combining acute accent (U+0301) -- renders identically,
        // different bytes.
        assert_eq!(
            to_canonical_json(&Value::Str("cafe\u{0301}".into())).unwrap(),
            b"\"cafe\xcc\x81\""
        );
    }

    // Every control character below U+0020, plus the two short escapes Go
    // uses that aren't \uXXXX, verified empirically against encoding/json in
    // this session (Go container, json.Marshal over every 0x00-0x1F byte).
    #[test]
    fn control_character_escaping_matches_go_json_marshal() {
        let cases: &[(char, &str)] = &[
            ('\u{00}', "\\u0000"),
            ('\u{07}', "\\u0007"),
            ('\u{08}', "\\b"),
            ('\u{09}', "\\t"),
            ('\u{0A}', "\\n"),
            ('\u{0B}', "\\u000b"),
            ('\u{0C}', "\\f"),
            ('\u{0D}', "\\r"),
            ('\u{0E}', "\\u000e"),
            ('\u{1F}', "\\u001f"),
        ];
        for (ch, escaped) in cases {
            let want = format!("\"{escaped}\"");
            assert_eq!(json(&Value::Str(ch.to_string())), want, "char {ch:?}");
        }
        // DEL (0x7F) is NOT escaped by Go's json.Marshal -- verified
        // empirically -- and must not be escaped here either.
        assert_eq!(
            to_canonical_json(&Value::Str('\u{7F}'.to_string())).unwrap(),
            [b'"', 0x7F, b'"']
        );
    }

    #[test]
    fn digest_is_sha256_with_lowercase_hex_prefix() {
        let d = digest(b"{}");
        assert!(d.starts_with("sha256:"));
        assert_eq!(d.len(), "sha256:".len() + 64);
        assert!(d["sha256:".len()..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && (b.is_ascii_digit() || b.is_ascii_lowercase())));
        // Deterministic, and sensitive to its input -- not a placeholder.
        assert_eq!(digest(b"{}"), digest(b"{}"));
        assert_ne!(digest(b"{}"), digest(b"[]"));
    }

    // A3's big.Int-equivalent case (review-caught on PR #19, closed by
    // adding reader.rs's Value::BigInt -- see that type's own doc comment).
    // Every vector the review named: the i64 boundary on both sides, zero/
    // sign/leading-zero normalization, and a value no machine integer could
    // ever hold -- normalized exactly like Go's big.Int.SetString(s, 10),
    // verified empirically against it in this session.
    #[test]
    fn big_integers_normalize_like_go_big_int() {
        let cases: &[(&str, &str)] = &[
            ("9223372036854775808", "9223372036854775808"),
            ("9223372036854775809", "9223372036854775809"),
            ("-9223372036854775809", "-9223372036854775809"),
            ("000000123", "123"),
            ("-00000123", "-123"),
            ("0", "0"),
            ("-0", "0"),
            ("+0", "0"),
            ("+123", "123"),
            (
                "1234567890123456789012345678901234567890",
                "1234567890123456789012345678901234567890",
            ),
            (
                "-1234567890123456789012345678901234567890",
                "-1234567890123456789012345678901234567890",
            ),
        ];
        for (literal, want) in cases {
            assert_eq!(
                json(&Value::BigInt((*literal).to_string())),
                *want,
                "literal {literal:?}"
            );
        }
    }

    // End-to-end differential test (the review's own ask): parse() through
    // to_canonical_json(), compared byte-for-byte against Go's real
    // math/big.Int.SetString(s, 10).String() output for the identical
    // literals, run in a Go container in this session -- not just this
    // crate's own normalization function checked in isolation.
    #[test]
    fn large_integers_round_trip_through_parse_and_canonicalize_matching_go() {
        let yaml = "- 9223372036854775807\n- 9223372036854775808\n- 9223372036854775809\n- -9223372036854775808\n- -9223372036854775809\n- 9007199254740992\n- 9007199254740993\n- 000000123\n- -00000123\n- 0\n- -0\n- 1234567890123456789012345678901234567890\n";
        let value =
            crate::reader::parse(yaml.as_bytes(), crate::error::Stage::Read, "$", "document")
                .expect("parses");
        let got = json(&value);
        assert_eq!(got, "[9223372036854775807,9223372036854775808,9223372036854775809,-9223372036854775808,-9223372036854775809,9007199254740992,9007199254740993,123,-123,0,0,1234567890123456789012345678901234567890]");
    }
}
