//! Canonical byte representation — section A of `docs/MATERIALIZATION-SPEC.md`.
//!
//! One byte representation of a [`Value`] tree, for digests and caches.
//! Mirrors `core/nexus/iac/canonicaljson.go`/`digest.go` (`CIC-Relay`) byte
//! for byte — verified empirically against Go's `encoding/json` in this
//! session, not assumed from documentation. Number canonicalization (A3)
//! already has a proven Rust peer in the `cic-canonical` crate; the logic is
//! ported here rather than taken as a dependency, so this crate's
//! canonicalization stays self-contained and this engine does not acquire a
//! cross-repository build dependency on `CIC-Relay`.
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
//! Whether `capability`/`coverage`/`provenance` metadata is part of this
//! byte tree (B7) is a question about *what* `Value` must be able to
//! represent before canonicalizing, not about this module — this module
//! canonicalizes whatever `Value` tree it is handed.

use crate::reader::{Map, Value};
use sha2::{Digest as _, Sha256};

/// Canonical JSON bytes for `v` (A1–A6): compact, no inter-token whitespace,
/// object keys sorted by raw UTF-8 byte order, numbers per A3, strings
/// escaped per A4, arrays kept in source order.
#[must_use]
pub fn to_canonical_json(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_value(&mut out, v);
    out
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

fn write_value(out: &mut Vec<u8>, v: &Value) {
    match v {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        // Exact-digit rendering, no precision loss -- `reader.rs` already
        // bounds integer precision to i64 at parse time; canonicalization
        // does not widen or narrow that, only renders it.
        Value::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Value::Float(f) => out.extend_from_slice(canonical_float(*f).as_bytes()),
        Value::Str(s) => write_json_string(out, s),
        Value::Seq(items) => write_array(out, items),
        Value::Map(m) => write_map(out, m),
    }
}

/// A3, ported from `cic-canonical`'s own `canonical_float` (not imported as a
/// dependency — see module docs): shortest round-trip plain-decimal form,
/// `-0.0` folded to `0`. Rust's `f64` `Display` is shortest round-trip and
/// never uses an exponent, matching Go's `FormatFloat(f, 'f', -1, 64)` —
/// verified empirically in this session (`4.0` -> `"4"`, `1000.0` ->
/// `"1000"`, not `"1e3"`).
fn canonical_float(f: f64) -> String {
    let s = format!("{f}");
    if s == "-0" {
        "0".to_string()
    } else {
        s
    }
}

fn write_array(out: &mut Vec<u8>, items: &[Value]) {
    out.push(b'[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(b',');
        }
        write_value(out, item);
    }
    out.push(b']');
}

fn write_map(out: &mut Vec<u8>, m: &Map) {
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
        );
    }
    out.push(b'}');
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
        String::from_utf8(to_canonical_json(v)).expect("canonical output is always valid UTF-8")
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
            to_canonical_json(&Value::Str("café".into())),
            b"\"caf\xc3\xa9\""
        );
        // "cafe" + combining acute accent (U+0301) -- renders identically,
        // different bytes.
        assert_eq!(
            to_canonical_json(&Value::Str("cafe\u{0301}".into())),
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
            to_canonical_json(&Value::Str('\u{7F}'.to_string())),
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
}
