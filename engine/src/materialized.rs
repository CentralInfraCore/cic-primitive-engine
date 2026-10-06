//! The materialized object -- `docs/MATERIALIZATION-SPEC.md` B1-B8
//! (CLOSED), the obligation `lib.rs`'s "Division of labor" scope
//! correction named but had not yet built: the custody boundary that
//! rejects a candidate before canonicalization or proof ever treats it as
//! real.
//!
//! # `FieldEvidence` -- closing a tension inside B3's own closed text
//!
//! B3's ASCII sketch shows `capability`/`coverage`/`provenance` as three
//! unconditional fields. B3's own prose contradicts that literal shape: *"a
//! field materialized from plain authored intent still carries a
//! provenance and no coverage; a field materialized from a raw,
//! non-derived observation still carries a coverage and no provenance.
//! Only the derived case populates both."* Two independent `Option`
//! fields (`Option<Coverage>`, `Option<Provenance>`) would still permit
//! `None`/`None`, a combination the prose never describes as legitimate.
//! [`FieldEvidence`] closes that: exactly the three combinations the prose
//! describes are the three variants, and nothing else type-checks.
//!
//! [`FieldEvidence::Observation`] and [`FieldEvidence::DerivedObservation`]
//! reuse [`crate::plan::ConsumedField`] rather than pairing a bare
//! `Option<Value>` with [`Coverage`] a second time -- the exact "forbidden
//! pairing, constructible anyway" defect PR #29 already found and fixed
//! once in `plan.rs`. Reusing the type here means there is no second place
//! for the same defect to reappear.
//!
//! [`FieldEvidence::DerivedObservation`] carries only a [`Coverage`], not a
//! [`Provenance`]: B3 ties `derived` crossing the intent/state line
//! specifically to the state-side case (`BOUNDARY.md`'s own
//! `$.state.effective_state` example), and `authored`/`schema_default`
//! never occur state-side (`schema_default` is forbidden on `authority:
//! state` by B1's own defaultability rule; `authored` presupposes an
//! operator wrote it, which an observation is not) -- so a state-side
//! derived field's provenance is always exactly `Derived`. Naming that
//! fact in the variant, rather than storing a free [`Provenance`] field
//! that could disagree with it, leaves nothing to get wrong.
//!
//! **Deliberately not resolved here:** whether an `authored`-provenance
//! field with no value (`PRIMITIVE-IR.md`'s "authored-absent") is
//! distinguishable from a legally null-valued authored field is a
//! representation question B2 explicitly left open ("a representation
//! decision this section does not make"). [`FieldEvidence::Intent`] keeps
//! `value: Option<Value>` rather than a non-optional `Value`, so this does
//! not silently close that question by picking a shape that only works if
//! it were already settled.
//!
//! # The custody boundary -- [`MaterializedObject::try_new`]
//!
//! This crate does not know what a schema's key set IS -- that is
//! domain-semantic knowledge, squarely the environment's job per the scope
//! correction. What it owns is narrower: given a candidate map AND the key
//! set the environment itself claims the schema declares, enforce that the
//! two actually match -- `PRIMITIVE-IR.md`'s Complete property, by
//! construction, not by convention. Per-field evidence-shape invariants
//! need no separate check here; [`FieldEvidence`]'s own type already makes
//! the illegal shapes unrepresentable.

use crate::error::{code, Error, Result, Stage};
use crate::plan::ConsumedField;
use crate::reader::Value;
use std::collections::{BTreeMap, BTreeSet};

/// B1's capability axis -- `access.yaml`'s own `conformance` field, named
/// for the axis (B1) rather than the schema field, to keep it apart from
/// this crate's own `conformance` module and F6-F9's comparator
/// vocabulary, which is a different "conformance" entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Implemented,
    NotImplemented,
    Deprecated,
}

impl Capability {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Implemented => "implemented",
            Capability::NotImplemented => "not_implemented",
            Capability::Deprecated => "deprecated",
        }
    }
}

/// B1's provenance axis -- intent-side only. See the module doc comment
/// for why a state-side derived field never stores this directly, even
/// though `derived` itself is "either side" per B1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    Authored,
    SchemaDefault,
    Derived,
}

impl Provenance {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Provenance::Authored => "authored",
            Provenance::SchemaDefault => "schema_default",
            Provenance::Derived => "derived",
        }
    }
}

/// B3's three legitimate field shapes, closed so a fourth, illegitimate
/// one cannot be constructed. See the module doc comment for the
/// reasoning behind each variant's exact shape.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldEvidence {
    /// Intent-side: operator-authored, schema-default-substituted, or
    /// computed from other authored fields. Never carries a `Coverage` --
    /// coverage is an observe-call concept, and an intent field is not
    /// observed.
    Intent {
        provenance: Provenance,
        value: Option<Value>,
    },
    /// State-side, not derived: a raw provider/device observation.
    Observation(ConsumedField),
    /// State-side, derived (`BOUNDARY.md`'s `$.state.effective_state`
    /// example). Provenance is always exactly `Derived` here -- see the
    /// module doc comment.
    DerivedObservation(ConsumedField),
}

/// B3's closed `MaterializedField` shape: `capability` (B1, always
/// present -- a static per-field/device-binding fact) plus
/// [`FieldEvidence`] (the coverage/provenance/value facts, whose legal
/// combinations `FieldEvidence`'s own variants already close).
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedField {
    pub capability: Capability,
    pub evidence: FieldEvidence,
}

impl MaterializedField {
    /// The field's materialized value, if it has one -- `None` for
    /// `Absent`/`Unobserved`/`Unknown` coverage, for an authored-absent
    /// intent field (B2, still open -- see module doc comment), and for
    /// nothing else. Derived from `evidence`, not a second, independently
    /// settable field -- the same reasoning `ConsumedField` itself already
    /// applies one layer down.
    #[must_use]
    pub fn value(&self) -> Option<&Value> {
        match &self.evidence {
            FieldEvidence::Intent { value, .. } => value.as_ref(),
            FieldEvidence::Observation(cf) | FieldEvidence::DerivedObservation(cf) => cf.value(),
        }
    }
}

/// B7/F13-F14's named custody-boundary obligation (`lib.rs`'s "Division of
/// labor" correction): exactly the schema's keys, no more and no less
/// (`PRIMITIVE-IR.md`'s own Complete property), enforced by construction.
/// See the module doc comment for what this crate does and does not know
/// going into that check.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedObject(BTreeMap<String, MaterializedField>);

impl MaterializedObject {
    /// Accepts `fields` as a genuine `MaterializedObject` only if its key
    /// set is EXACTLY `expected_keys` -- not a subset, not a superset.
    /// `expected_keys` is the caller's (the environment's) own claim about
    /// what the schema declares; this crate does not independently verify
    /// that claim against any schema (out of scope, per the scope
    /// correction), only that the candidate actually matches whatever
    /// claim it was handed.
    pub fn try_new(
        fields: BTreeMap<String, MaterializedField>,
        expected_keys: &BTreeSet<String>,
    ) -> Result<Self> {
        let actual: BTreeSet<&String> = fields.keys().collect();
        let expected: BTreeSet<&String> = expected_keys.iter().collect();

        if actual != expected {
            let missing: Vec<&str> = expected.difference(&actual).map(|s| s.as_str()).collect();
            let extra: Vec<&str> = actual.difference(&expected).map(|s| s.as_str()).collect();
            return Err(Error::new(
                code::INCOMPLETE_OBJECT,
                "R-COMPLETE",
                Stage::Materialize,
                "$",
                format!("key set does not match: missing={missing:?}, extra={extra:?}"),
            ));
        }

        Ok(Self(fields))
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&MaterializedField> {
        self.0.get(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authored(v: Value) -> MaterializedField {
        MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Intent {
                provenance: Provenance::Authored,
                value: Some(v),
            },
        }
    }

    fn observed(v: Value) -> MaterializedField {
        MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Observed(v)),
        }
    }

    #[test]
    fn axis_strings_match_the_schema_and_boundary_names() {
        assert_eq!(Capability::Implemented.as_str(), "implemented");
        assert_eq!(Capability::NotImplemented.as_str(), "not_implemented");
        assert_eq!(Capability::Deprecated.as_str(), "deprecated");
        assert_eq!(Provenance::Authored.as_str(), "authored");
        assert_eq!(Provenance::SchemaDefault.as_str(), "schema_default");
        assert_eq!(Provenance::Derived.as_str(), "derived");
    }

    // B3: "a field materialized from plain authored intent still carries a
    // provenance and no coverage" -- value() must read straight through
    // the Intent variant's own Option, no fallback invented.
    #[test]
    fn intent_value_reads_through_directly() {
        let f = authored(Value::Str("prod".into()));
        assert_eq!(f.value(), Some(&Value::Str("prod".into())));
    }

    // B2's still-open authored-absent question: a None value under
    // Provenance::Authored must stay representable and readable as
    // "no value," not panic or get coerced into something else.
    #[test]
    fn authored_absent_has_no_value_and_does_not_panic() {
        let f = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Intent {
                provenance: Provenance::Authored,
                value: None,
            },
        };
        assert_eq!(f.value(), None);
    }

    // B3: "a field materialized from a raw, non-derived observation still
    // carries a coverage and no provenance" -- and the value/coverage
    // pairing is ConsumedField's own invariant, not re-checked here.
    #[test]
    fn observation_value_comes_from_consumed_field() {
        let f = observed(Value::Int(16));
        assert_eq!(f.value(), Some(&Value::Int(16)));

        let f = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::Observation(ConsumedField::Absent),
        };
        assert_eq!(f.value(), None);
    }

    // B3: "only the derived case populates both" -- DerivedObservation
    // carries a real value when Observed, and Derived provenance is
    // implied by the variant itself, not a field that could disagree.
    #[test]
    fn derived_observation_value_comes_from_consumed_field_too() {
        let f = MaterializedField {
            capability: Capability::Implemented,
            evidence: FieldEvidence::DerivedObservation(ConsumedField::Observed(Value::Str(
                "RUNNING".into(),
            ))),
        };
        assert_eq!(f.value(), Some(&Value::Str("RUNNING".into())));
    }

    fn keys(ks: &[&str]) -> BTreeSet<String> {
        ks.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn try_new_accepts_an_exact_key_match() {
        let mut fields = BTreeMap::new();
        fields.insert("shape".to_string(), authored(Value::Str("E4.Flex".into())));
        fields.insert("memory_gb".to_string(), observed(Value::Int(16)));

        let obj = MaterializedObject::try_new(fields, &keys(&["shape", "memory_gb"]))
            .expect("exact key match must be accepted");
        assert_eq!(obj.len(), 2);
        assert!(!obj.is_empty());
        assert!(obj.get("shape").is_some());
    }

    #[test]
    fn try_new_rejects_a_missing_key() {
        let mut fields = BTreeMap::new();
        fields.insert("shape".to_string(), authored(Value::Str("E4.Flex".into())));

        let err = MaterializedObject::try_new(fields, &keys(&["shape", "memory_gb"]))
            .expect_err("a candidate missing a declared key must be rejected");
        assert_eq!(err.code, code::INCOMPLETE_OBJECT);
        assert_eq!(err.stage, Stage::Materialize);
    }

    #[test]
    fn try_new_rejects_an_extra_key() {
        let mut fields = BTreeMap::new();
        fields.insert("shape".to_string(), authored(Value::Str("E4.Flex".into())));
        fields.insert("not_in_schema".to_string(), observed(Value::Bool(true)));

        let err = MaterializedObject::try_new(fields, &keys(&["shape"]))
            .expect_err("a candidate carrying an undeclared key must be rejected");
        assert_eq!(err.code, code::INCOMPLETE_OBJECT);
        assert_eq!(err.stage, Stage::Materialize);
    }

    #[test]
    fn try_new_accepts_the_empty_object() {
        let obj = MaterializedObject::try_new(BTreeMap::new(), &BTreeSet::new())
            .expect("zero declared keys and zero fields is a legitimate, if trivial, match");
        assert!(obj.is_empty());
    }
}
