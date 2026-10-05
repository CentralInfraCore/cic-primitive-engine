//! Collection topology — how a list field's elements are identified. Ported
//! from `CIC-Relay`'s `core/nexus/iac/collection.go`, taken (there,
//! deliberately, not reinvented) from Kubernetes server-side-apply: a list
//! is atomic, a set, or a map keyed by named fields. Needed by section F's
//! object-level comparator walk (`conformance.go`'s `elementKeys`, not yet
//! ported) to tell collection elements on the intent and observed sides
//! apart.
//!
//! # `ElementKey` parity with Go's `fmt.Sprintf("%v", ...)`
//!
//! **Correction (review-caught on PR #27): an earlier version of this
//! module refused to format `Float`/`Seq`/`Map` values at all, on the
//! reasoning that an identity should never be "guessed."** That reasoning
//! held for a map-topology *key field* (this document's own `BOUNDARY.md`
//! defaultability table: *"identity is never guessed"*), but not for a
//! `TopologySet` element: there, the *whole value* already **is** the
//! identity by construction — there is nothing to guess, only something to
//! format, exactly as Go's reference does unconditionally. Refusing to
//! format a `Float`/`Seq`/`Map` *element* was simply narrower than the Go
//! reference it ports, for no principled reason distinct from rarity — the
//! same category of gap already rejected once this session for the numeric
//! comparator's string grammar (PR #25).
//!
//! Fixed: [`Collection::element_key`] now reproduces `fmt.Sprintf("%v",
//! ...)` for every `Value` variant, including `Float` (Go's `%v` for
//! `float64` is `strconv.FormatFloat(f, 'g', -1, 64)` — plain decimal for
//! an exponent of -4..5, scientific otherwise, verified empirically across
//! many magnitudes and significant-digit counts in a Docker container, a
//! *different*, non-trivial algorithm from section A's own
//! `canonical_float`, which never uses scientific notation at all) and
//! `Seq`/`Map` (Go's `%v` prints `"[e1 e2 e3]"` for a slice and `"map[k1:v1
//! k2:v2]"` for a map, keys sorted — Go's `fmt` package sorts map keys for
//! deterministic `%v` output since Go 1.12, verified empirically, which
//! happens to be exactly this crate's own A2 key-sort rule, reused here,
//! not reinvented). The one residual gap is `NaN`/`±Inf`, which `%v` would
//! print as `"NaN"`/`"+Inf"`/`"-Inf"` and this module refuses (no stable
//! identity) — out of scope for a *materialized* value in this engine's own
//! pipeline (section A's `Stage::Canonicalize` already rejects non-finite
//! floats), kept here only as a defensive, documented exception for a
//! `Value` this function might be handed before that stage runs.

use crate::canonical::canonical_integer;
use crate::reader::Value;

/// Ported from `collection.go`'s `CollectionTopology`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CollectionTopology {
    /// The whole list is a single opaque value (one owner).
    #[default]
    Atomic,
    /// Elements are unique by value; order is irrelevant.
    Set,
    /// Elements are identified by one or more key fields.
    Map,
}

impl CollectionTopology {
    /// `collection.go`'s own three `CollectionTopology` string constants
    /// (`"atomic"`/`"set"`/`"map"`) -- the exact literal F5's
    /// `PlanDigestProjection` commits to per collection.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CollectionTopology::Atomic => "atomic",
            CollectionTopology::Set => "set",
            CollectionTopology::Map => "map",
        }
    }
}

/// Ported from `collection.go`'s `Collection`. `keys` applies only to
/// `CollectionTopology::Map`.
#[derive(Debug, Clone, Default)]
pub struct Collection {
    pub topology: CollectionTopology,
    pub keys: Vec<String>,
}

impl Collection {
    /// Ported from `collection.go`'s `ElementKey`: a stable identity for a
    /// list element under this topology.
    ///   - `Map`: the sorted `"k=v"` tuple of the element's key fields
    ///     (`"name=nic-0"`); a missing key field renders as `"<nil>"`,
    ///     matching Go's `fmt.Sprintf("%v", nil)` exactly (verified
    ///     empirically) — not an empty string, and not omitted.
    ///   - `Set`: a string form of the element value, matching
    ///     `fmt.Sprintf("%v", elem)` in full (see module docs) — Go's own
    ///     comment still calls this "a placeholder identity until the CIC
    ///     Canonical Object Encoding lands," a property of the reference
    ///     being ported, not of how faithfully it's ported here.
    ///   - `Atomic` (or the zero value): `""` — the list has no
    ///     per-element identity, it is one value.
    #[must_use]
    pub fn element_key(&self, elem: &Value) -> String {
        match self.topology {
            CollectionTopology::Map => {
                let Value::Map(m) = elem else {
                    return String::new(); // "map topology, non-map element -> no key"
                };
                let mut keys: Vec<&str> = self.keys.iter().map(String::as_str).collect();
                // Stable regardless of declared order -- matches Go's own
                // `sort.Strings(keys)`, not reinvented here.
                keys.sort_unstable();
                let mut parts = Vec::with_capacity(keys.len());
                for k in keys {
                    let display = match m.get(k) {
                        Some(v) => match go_display(v) {
                            Some(s) => s,
                            // Only a non-finite float reaches this branch
                            // (see module docs) -- no identity, not a
                            // guessed one.
                            None => return String::new(),
                        },
                        None => "<nil>".to_string(),
                    };
                    parts.push(format!("{k}={display}"));
                }
                parts.join(",")
            }
            CollectionTopology::Set => go_display(elem).unwrap_or_default(),
            CollectionTopology::Atomic => String::new(),
        }
    }
}

/// `fmt.Sprintf("%v", ...)`, ported in full (see this module's own doc
/// comment for the correction history). `None` only for a non-finite
/// float -- every other `Value` variant, including nested `Seq`/`Map`, has
/// a defined, Go-matching display form. `pub(crate)`: the not-yet-built
/// object-level walker's `resolvePath`-equivalent needs the exact same
/// formatting for its own `{key=val}` path-segment matching (`conformance.
/// go`'s `resolvePath` uses the identical `fmt.Sprintf("%v", em[key])`),
/// and must not grow a second, drifting copy of it.
pub(crate) fn go_display(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some("<nil>".to_string()),
        Value::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::BigInt(s) => canonical_integer(s),
        Value::Float(f) => go_float_display(*f),
        Value::Str(s) => Some(s.clone()),
        Value::Seq(items) => {
            let mut parts = Vec::with_capacity(items.len());
            for item in items {
                parts.push(go_display(item)?);
            }
            Some(format!("[{}]", parts.join(" ")))
        }
        Value::Map(m) => {
            let mut keys = m.keys();
            // Go's `fmt` package has sorted map keys for deterministic
            // `%v` output since Go 1.12 (verified empirically) -- A2's
            // byte-wise sort, reused, not a second rule.
            keys.sort_unstable();
            let mut parts = Vec::with_capacity(keys.len());
            for k in keys {
                let val = m.get(k).expect("key came from this map's own key list");
                parts.push(format!("{k}:{}", go_display(val)?));
            }
            Some(format!("map[{}]", parts.join(" ")))
        }
    }
}

/// `fmt.Sprintf("%v", f)` for a `float64` -- equivalent to
/// `strconv.FormatFloat(f, 'g', -1, 64)` (verified empirically in this
/// session: both produce byte-identical output for every vector tested).
/// Plain decimal when the decimal exponent of the leading significant
/// digit is in `-4..=5`; scientific notation (`"d.ddde±NN"`, exponent
/// zero-padded to at least two digits, explicit sign) otherwise -- a
/// *different* threshold from section A's `canonical_float`, which never
/// uses scientific notation at all, verified by sweeping exponents from
/// -8 to 25 across multiple significant-digit counts in a Docker
/// container: the threshold held at exactly this boundary regardless of
/// how many significant digits the value had.
///
/// `None` for `NaN`/`±Infinity` -- `%v` would print `"NaN"`/`"+Inf"`/
/// `"-Inf"`, but a materialized `Value` in this engine's own pipeline
/// never carries one (section A's `Stage::Canonicalize` already rejects
/// non-finite floats); kept as a defensive, documented exception for a
/// `Value` handed to this function before that stage runs.
fn go_float_display(f: f64) -> Option<String> {
    if !f.is_finite() {
        return None;
    }
    if f == 0.0 {
        // Correction (found while building this crate's Go peer,
        // go/collection, PR #32): unlike `canonical_float` (section A, a
        // DIFFERENT algorithm, not reused here -- see this module's own
        // doc comment), `fmt.Sprintf("%v", ...)` does NOT fold negative
        // zero. Verified empirically: real Go's `json.Unmarshal("-0.0",
        // &f)` followed by `fmt.Sprintf("%v", f)` prints "-0", not "0" --
        // the sign survives decoding. This function's own early return
        // used to fold both signs to "0" unconditionally, which this
        // crate's own test suite never caught because nobody had run
        // this specific vector against real Go output before encoding
        // it; `is_sign_negative()` distinguishes the two IEEE754 zeros
        // even though `f == 0.0` is true for both.
        return Some(if f.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        });
    }
    // Rust's `{:e}` is exactly the normalized form this needs: one
    // nonzero digit before the point, the shortest round-trip digit
    // string after it, and the decimal exponent of that leading digit --
    // the same quantity Go's algorithm thresholds on.
    let sci = format!("{f:e}");
    let (mantissa_part, exp_part) = sci
        .split_once('e')
        .expect("Rust's `{:e}` formatting always contains 'e'");
    let exp: i32 = exp_part
        .parse()
        .expect("the exponent after 'e' is always a valid integer");
    let neg = mantissa_part.starts_with('-');
    let digits: String = mantissa_part
        .trim_start_matches('-')
        .chars()
        .filter(|c| *c != '.')
        .collect();

    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if !(-4..6).contains(&exp) {
        out.push(digits.as_bytes()[0] as char);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if exp >= 0 { '+' } else { '-' });
        out.push_str(&format!("{:02}", exp.unsigned_abs()));
    } else if exp >= 0 {
        let int_len = (exp + 1) as usize;
        if digits.len() <= int_len {
            out.push_str(&digits);
            out.push_str(&"0".repeat(int_len - digits.len()));
        } else {
            out.push_str(&digits[..int_len]);
            out.push('.');
            out.push_str(&digits[int_len..]);
        }
    } else {
        out.push_str("0.");
        out.push_str(&"0".repeat((-exp - 1) as usize));
        out.push_str(&digits);
    }
    Some(out)
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

    fn keys(ks: &[&str]) -> Vec<String> {
        ks.iter().map(|s| s.to_string()).collect()
    }

    // Mirrors TestCollection_ElementKey in collection_test.go, case for
    // case, against the real Go vectors, not invented ones.
    #[test]
    fn element_key_mirrors_go() {
        let map_coll = Collection {
            topology: CollectionTopology::Map,
            keys: keys(&["name"]),
        };
        assert_eq!(
            map_coll.element_key(&map(&[
                ("name", Value::Str("nic-0".into())),
                ("subnet", Value::Str("a".into())),
            ])),
            "name=nic-0"
        );

        // multi-key: stable, sorted regardless of declared order.
        let multi = Collection {
            topology: CollectionTopology::Map,
            keys: keys(&["zone", "name"]),
        };
        assert_eq!(
            multi.element_key(&map(&[
                ("name", Value::Str("nic-0".into())),
                ("zone", Value::Str("eu".into())),
            ])),
            "name=nic-0,zone=eu"
        );

        // map topology, non-map element -> no key.
        assert_eq!(map_coll.element_key(&Value::Str("scalar".into())), "");

        // missing key field rendered explicitly, not omitted.
        assert_eq!(
            map_coll.element_key(&map(&[("subnet", Value::Str("a".into()))])),
            "name=<nil>"
        );

        // set -> value form.
        assert_eq!(
            Collection {
                topology: CollectionTopology::Set,
                keys: vec![],
            }
            .element_key(&Value::Str("10.0.0.0/16".into())),
            "10.0.0.0/16"
        );

        // atomic / unset -> no per-element identity.
        assert_eq!(
            Collection {
                topology: CollectionTopology::Atomic,
                keys: vec![],
            }
            .element_key(&Value::Str("x".into())),
            ""
        );
        assert_eq!(
            Collection::default().element_key(&Value::Str("x".into())),
            ""
        );
    }

    // Review-caught on PR #27: a float-valued key field, or a whole
    // TopologySet element, is NOT a guessed identity -- Go already assigns
    // one via %v, and this port must too. Every vector here was generated
    // by running real Go fmt.Sprintf("%v", ...) in a Docker container in
    // this session, not transcribed from memory.
    #[test]
    fn float_values_get_the_same_identity_go_does() {
        let set = Collection {
            topology: CollectionTopology::Set,
            keys: vec![],
        };
        let cases: &[(f64, &str)] = &[
            (16.0, "16"),
            (16.5, "16.5"),
            (0.1, "0.1"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (100000.0, "100000"),
            (1_000_000.0, "1e+06"),
            (123456.0, "123456"),
            (1_234_567.0, "1.234567e+06"),
            (1e20, "1e+20"),
            (1e-100, "1e-100"),
            (-16.5, "-16.5"),
            (0.0, "0"),
        ];
        for (f, want) in cases {
            assert_eq!(set.element_key(&Value::Float(*f)), *want, "{f}");
        }

        let map_coll = Collection {
            topology: CollectionTopology::Map,
            keys: keys(&["id"]),
        };
        assert_eq!(
            map_coll.element_key(&map(&[("id", Value::Float(1.5))])),
            "id=1.5"
        );

        // NaN/Infinity: the one residual, defensively-handled exception.
        assert_eq!(set.element_key(&Value::Float(f64::NAN)), "");
        assert_eq!(set.element_key(&Value::Float(f64::INFINITY)), "");
    }

    // Correction (found while building this crate's Go peer,
    // go/collection, PR #32, not caught by review on PR #27): this
    // module's own `go_float_display` used to fold -0.0 to "0"
    // unconditionally, on the assumption that `fmt.Sprintf("%v", ...)`
    // treats zero the way section A's own `canonical_float` does. It
    // does not. Verified empirically: real Go's
    // `json.Unmarshal([]byte("-0.0"), &f)` followed by
    // `fmt.Sprintf("%v", f)` prints "-0" -- the sign survives decoding,
    // because `f == 0.0` is true for both IEEE754 zeros but they are
    // not the same bit pattern. The previous test vector asserted
    // `(-0.0, "0")`, which was simply wrong and went uncaught because
    // nobody had actually run this one vector against real Go output
    // before encoding it, despite this test's own comment claiming
    // every vector had been.
    #[test]
    fn negative_zero_keeps_its_sign_unlike_canonical_float() {
        let set = Collection {
            topology: CollectionTopology::Set,
            keys: vec![],
        };
        assert_eq!(set.element_key(&Value::Float(0.0_f64)), "0");
        assert_eq!(set.element_key(&Value::Float(-0.0_f64)), "-0");
        // Authored YAML text parses to a genuine sign-preserving negative
        // zero too, not just the literal -- the same precision-and-sign
        // fidelity this crate's own big-integer handling already insists
        // on elsewhere (Value::BigInt).
        let parsed: f64 = "-0.0".parse().expect("valid float literal");
        assert!(parsed.is_sign_negative());
        assert_eq!(set.element_key(&Value::Float(parsed)), "-0");
    }

    // Review-caught on PR #27: a TopologySet element can itself be a
    // nested Seq/Map, matching Go's %v exactly ("[e1 e2]" / "map[k:v]",
    // keys sorted) -- not refused as "no identity" the way the first
    // version of this module did.
    #[test]
    fn nested_seq_and_map_values_match_go_v_formatting() {
        let set = Collection {
            topology: CollectionTopology::Set,
            keys: vec![],
        };
        assert_eq!(
            set.element_key(&Value::Seq(vec![
                Value::Str("a".into()),
                Value::Int(1),
                Value::Bool(true),
            ])),
            "[a 1 true]"
        );
        // Go sorts map keys for %v regardless of insertion order.
        assert_eq!(
            set.element_key(&map(
                &[("b", Value::Int(1)), ("a", Value::Str("x".into())),]
            )),
            "map[a:x b:1]"
        );
        assert_eq!(set.element_key(&Value::Seq(vec![])), "[]");
        assert_eq!(set.element_key(&map(&[])), "map[]");
    }
}
