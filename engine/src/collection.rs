//! Collection topology — how a list field's elements are identified. Ported
//! from `CIC-Relay`'s `core/nexus/iac/collection.go`, taken (there,
//! deliberately, not reinvented) from Kubernetes server-side-apply: a list
//! is atomic, a set, or a map keyed by named fields. Needed by section F's
//! object-level comparator walk (`conformance.go`'s `elementKeys`, not yet
//! ported) to tell collection elements on the intent and observed sides
//! apart.
//!
//! # A deliberate, bounded gap: floating-point and non-scalar key values
//!
//! Go's `ElementKey` formats *any* key-field or set-element value via
//! `fmt.Sprintf("%v", ...)`, including `float64` (scientific notation above
//! or below certain magnitudes — a *different*, non-trivial formatting
//! algorithm from section A's own `canonical_float`, verified empirically
//! in this session: Go's `%v` prints `1e+20`, where `canonical_float`
//! prints the full plain-decimal digit run) and arbitrary maps/slices.
//! [`Collection::element_key`] refuses both (returns `""`, meaning "no
//! stable identity"), narrower than Go. This is not excused by rarity
//! alone — that argument was already rejected once this session, for the
//! numeric comparator's string grammar (PR #25), and rightly so, since a
//! *general-purpose* comparator has no principled reason to exclude any
//! input shape. An identity *key field* is different in kind: this
//! document's own `BOUNDARY.md` already holds (defaultability table,
//! `structural: key`) that *"identity is never guessed"* — a schema that
//! identifies a collection element by a float is already in tension with
//! that principle, independent of this port. Go's own `collection.go`
//! comment calls its `TopologySet` `ElementKey` itself *"a placeholder
//! identity until the CIC Canonical Object Encoding lands"* — i.e. the
//! reference this ports is explicit that the set case isn't a finished
//! contract either. Porting `%v`'s exact float/map/slice algorithm to
//! match an admittedly-provisional upstream shape is deferred, not
//! silently absent.

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
    ///   - `Set`: a string form of the element value (Go's own comment:
    ///     "a placeholder identity until the CIC Canonical Object Encoding
    ///     lands" — see this module's doc comment).
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
                            // A value this function can't confidently
                            // display (see module docs) -- no identity,
                            // not a guessed one.
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

/// `fmt.Sprintf("%v", ...)`'s result for the `Value` variants this module
/// can confidently reproduce byte-for-byte (verified empirically against
/// real Go output): `nil` -> `"<nil>"`, a bool -> `"true"`/`"false"`, a
/// string -> itself, verbatim, and an integer -> its decimal digits
/// (`BigInt` via section A's own `canonical_integer`, the same
/// normalization Go's own big-integer display would apply). `None` for
/// `Float`/`Seq`/`Map` -- see this module's own doc comment for why.
fn go_display(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some("<nil>".to_string()),
        Value::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::BigInt(s) => canonical_integer(s),
        Value::Str(s) => Some(s.clone()),
        Value::Float(_) | Value::Seq(_) | Value::Map(_) => None,
    }
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

    // Not in collection_test.go -- this engine's own named gap (see module
    // docs), pinned down as a real test rather than only a doc claim.
    #[test]
    fn float_and_non_scalar_key_values_have_no_identity() {
        let map_coll = Collection {
            topology: CollectionTopology::Map,
            keys: keys(&["id"]),
        };
        assert_eq!(
            map_coll.element_key(&map(&[("id", Value::Float(1.5))])),
            "",
            "a float-valued key field yields no identity, not a guessed one"
        );
        assert_eq!(
            Collection {
                topology: CollectionTopology::Set,
                keys: vec![],
            }
            .element_key(&Value::Float(1.5)),
            ""
        );
    }
}
