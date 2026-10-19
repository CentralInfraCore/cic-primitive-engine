//! A verifier for the materialization receipt (`docs/RECEIPT-SCHEMA.md`).
//!
//! This module checks a receipt that somebody else produced. It never
//! produces one: materialization, and the evidence a default or a derivation
//! leaves while it runs, belong to the environment (`docs/BOUNDARY.md`).
//!
//! # What a successful result establishes
//!
//! **Structural and content consistency, and nothing more.** A receipt that
//! passes [`verify_receipt`]:
//!
//! * has all 15 required top-level fields, with the right types, a supported
//!   `receipt_schema_version` (`1`) and every digest spelled `sha256:` + 64
//!   lowercase hex characters;
//! * lists `applied_defaults` and `derived_values` in strictly ascending
//!   `.path` order, and each `inputs` list as a strictly ascending
//!   (deduplicated) path set;
//! * carries an `input_digest` equal to the A7 digest of the canonical form of
//!   the authored input it was handed, and an `output_digest` equal to
//!   [`materialized_object_digest`] of the object it was handed;
//! * lists exactly the defaulted and the derived fields of that object, each
//!   with the digest of the field's value, **through the explicit
//!   [`PathBinding`] the caller supplied**.
//!
//! It does **not** establish that a default was applied or a derivation ran,
//! that `rule` or `inputs` describe what actually happened, who produced the
//! receipt, or that any identifier is genuine:
//!
//! * `grammar_*`, `primitive_*`, `schema_*` and `engine_*` are claims. Their
//!   form is checked; the release signature and the origin of the pinned
//!   schema are not (external authentication is not this module's job).
//! * `engine_identity` names the implementation that **materialized** the
//!   data, not this verifier.
//! * No signature is checked. The receipt schema has no signature field.
//!
//! # `input_digest`
//!
//! The caller passes the processed authored input as a [`Value`]. The digest
//! is taken over that value's canonical JSON (section A), not over raw source
//! bytes, and no YAML reader is assumed.
//!
//! # Evidence needs an explicit path binding
//!
//! A `MaterializedObject` is keyed by flat field names. A receipt names fields
//! by path (`$.config.replicas`). This module does not guess a mapping from
//! one to the other. When the receipt lists any evidence entry, the caller
//! must pass a [`PathBinding`] from receipt path to object key; an entry whose
//! path is not in the binding is refused with `E_RECEIPT_UNBOUND`
//! (fail-closed), never accepted partially. With both evidence lists empty and
//! no defaulted or derived field in the object, no binding is needed.
//!
//! `derived_values[].inputs` is checked for form and order only. The finished
//! object cannot show which paths a derivation actually read, so no input path
//! is required to exist in the object.
//!
//! # Check order
//!
//! Shape and structure first (declared field order), then `input_digest`, then
//! `output_digest`, then evidence entries in list order, then the object's own
//! defaulted/derived fields in key order. The Go peer (`go/receipt`) runs the
//! same order, so both name the same offending location.

use std::collections::{BTreeMap, BTreeSet};

use crate::canonical::{digest, to_canonical_json};
use crate::digest_projection::materialized_object_digest;
use crate::error::{code, Error, Result, Stage};
use crate::materialized::{FieldEvidence, IntentEvidence, MaterializedField, MaterializedObject};
use crate::reader::Value;

/// The only `receipt_schema_version` this verifier supports.
pub const RECEIPT_SCHEMA_VERSION: i64 = 1;

/// `DefaultEvidence.rule`: a literal v1 constant (`RECEIPT-SCHEMA.md`, C5).
pub const DEFAULT_RULE: &str = "schema-default";

/// The sole wrapper key of a receipt document.
const WRAPPER: &str = "materialization";

/// The 15 required top-level fields, in declared order.
const FIELDS: [&str; 15] = [
    "receipt_schema_version",
    "grammar_sha256",
    "grammar_schema_sha256",
    "primitive_release",
    "primitive_source_commit",
    "primitive_build_hash",
    "schema_name",
    "schema_version",
    "schema_digest",
    "engine_identity",
    "engine_version",
    "input_digest",
    "output_digest",
    "applied_defaults",
    "derived_values",
];

/// Receipt path -> `MaterializedObject` key, supplied by the environment.
pub type PathBinding = BTreeMap<String, String>;

/// `applied_defaults[]` entry (`DefaultEvidence`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultEvidence {
    pub path: String,
    pub rule: String,
    pub value_digest: String,
}

/// `derived_values[]` entry (`DerivedEvidence`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedEvidence {
    pub path: String,
    pub rule: String,
    pub inputs: Vec<String>,
    pub value_digest: String,
}

/// The materialization receipt: 15 required fields, none optional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub receipt_schema_version: i64,
    pub grammar_sha256: String,
    pub grammar_schema_sha256: String,
    pub primitive_release: String,
    pub primitive_source_commit: String,
    pub primitive_build_hash: String,
    pub schema_name: String,
    pub schema_version: String,
    pub schema_digest: String,
    pub engine_identity: String,
    pub engine_version: String,
    pub input_digest: String,
    pub output_digest: String,
    pub applied_defaults: Vec<DefaultEvidence>,
    pub derived_values: Vec<DerivedEvidence>,
}

fn invalid(rule: &'static str, path: &str, detail: String) -> Error {
    Error::new(code::INVALID_RECEIPT, rule, Stage::Validate, path, detail)
}

fn mismatch(rule: &'static str, path: &str, detail: String) -> Error {
    Error::new(code::RECEIPT_MISMATCH, rule, Stage::Validate, path, detail)
}

fn field_path(name: &str) -> String {
    format!("$.{WRAPPER}.{name}")
}

fn shape(path: &str, want: &str) -> Error {
    invalid("R-RECEIPT-SHAPE", path, format!("expected {want}"))
}

fn take_str(m: &crate::reader::Map, key: &str, path: &str) -> Result<String> {
    match m.get(key) {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(_) => Err(shape(path, "a string")),
        None => Err(shape(path, "this required field (it is absent)")),
    }
}

fn take_map<'a>(v: &'a Value, path: &str) -> Result<&'a crate::reader::Map> {
    match v {
        Value::Map(m) => Ok(m),
        _ => Err(shape(path, "a mapping")),
    }
}

fn check_keys(m: &crate::reader::Map, allowed: &[&str], path: &str) -> Result<()> {
    let mut extra: Vec<&str> = m
        .keys()
        .into_iter()
        .filter(|k| !allowed.contains(k))
        .collect();
    extra.sort_unstable();
    match extra.first() {
        Some(k) => Err(shape(
            &format!("{path}.{k}"),
            "no member outside the receipt layout",
        )),
        None => Ok(()),
    }
}

impl Receipt {
    /// Reads a receipt document: a mapping whose only key is
    /// `materialization`, holding the 15 fields. Shape errors (missing or
    /// unknown member, wrong type) are raised here, in declared field order;
    /// form and order errors are raised by [`Receipt::check_structure`], which
    /// this calls last.
    ///
    /// # Errors
    /// `E_INVALID_RECEIPT`.
    pub fn from_value(doc: &Value) -> Result<Receipt> {
        let top = take_map(doc, "$")?;
        if top.get(WRAPPER).is_none() {
            return Err(shape("$", "the sole wrapper key `materialization`"));
        }
        check_keys(top, &[WRAPPER], "$")?;
        let base = format!("$.{WRAPPER}");
        let body = take_map(top.get(WRAPPER).expect("checked above"), &base)?;

        let mut version = 0_i64;
        let mut strings: BTreeMap<&str, String> = BTreeMap::new();
        let mut defaults = Vec::new();
        let mut derived = Vec::new();
        for name in FIELDS {
            let path = field_path(name);
            match name {
                "receipt_schema_version" => match body.get(name) {
                    Some(Value::Int(n)) => version = *n,
                    Some(_) => return Err(shape(&path, "an integer")),
                    None => return Err(shape(&path, "this required field (it is absent)")),
                },
                "applied_defaults" => match body.get(name) {
                    Some(Value::Seq(items)) => {
                        for (i, item) in items.iter().enumerate() {
                            defaults.push(parse_default(item, &format!("{path}[{i}]"))?);
                        }
                    }
                    Some(_) => return Err(shape(&path, "a sequence")),
                    None => return Err(shape(&path, "this required field (it is absent)")),
                },
                "derived_values" => match body.get(name) {
                    Some(Value::Seq(items)) => {
                        for (i, item) in items.iter().enumerate() {
                            derived.push(parse_derived(item, &format!("{path}[{i}]"))?);
                        }
                    }
                    Some(_) => return Err(shape(&path, "a sequence")),
                    None => return Err(shape(&path, "this required field (it is absent)")),
                },
                _ => {
                    strings.insert(name, take_str(body, name, &path)?);
                }
            }
        }
        check_keys(body, &FIELDS, &base)?;

        let mut s = |k: &str| {
            strings
                .remove(k)
                .expect("every string field was read above")
        };
        let receipt = Receipt {
            receipt_schema_version: version,
            grammar_sha256: s("grammar_sha256"),
            grammar_schema_sha256: s("grammar_schema_sha256"),
            primitive_release: s("primitive_release"),
            primitive_source_commit: s("primitive_source_commit"),
            primitive_build_hash: s("primitive_build_hash"),
            schema_name: s("schema_name"),
            schema_version: s("schema_version"),
            schema_digest: s("schema_digest"),
            engine_identity: s("engine_identity"),
            engine_version: s("engine_version"),
            input_digest: s("input_digest"),
            output_digest: s("output_digest"),
            applied_defaults: defaults,
            derived_values: derived,
        };
        receipt.check_structure()?;
        Ok(receipt)
    }

    /// Form, version and ordering checks that need no other artifact.
    ///
    /// # Errors
    /// `E_INVALID_RECEIPT`.
    pub fn check_structure(&self) -> Result<()> {
        if self.receipt_schema_version != RECEIPT_SCHEMA_VERSION {
            return Err(invalid(
                "R-RECEIPT-VERSION",
                &field_path("receipt_schema_version"),
                format!(
                    "unsupported receipt_schema_version {} (supported: {RECEIPT_SCHEMA_VERSION})",
                    self.receipt_schema_version
                ),
            ));
        }
        check_digest(&self.grammar_sha256, "grammar_sha256")?;
        check_digest(&self.grammar_schema_sha256, "grammar_schema_sha256")?;
        check_nonempty(&self.primitive_release, "primitive_release")?;
        check_nonempty(&self.primitive_source_commit, "primitive_source_commit")?;
        check_digest(&self.primitive_build_hash, "primitive_build_hash")?;
        check_nonempty(&self.schema_name, "schema_name")?;
        check_nonempty(&self.schema_version, "schema_version")?;
        check_digest(&self.schema_digest, "schema_digest")?;
        check_nonempty(&self.engine_identity, "engine_identity")?;
        check_nonempty(&self.engine_version, "engine_version")?;
        check_digest(&self.input_digest, "input_digest")?;
        check_digest(&self.output_digest, "output_digest")?;

        for (i, e) in self.applied_defaults.iter().enumerate() {
            let at = format!("{}[{i}]", field_path("applied_defaults"));
            check_path_form(&e.path, &format!("{at}.path"))?;
            if e.rule != DEFAULT_RULE {
                return Err(invalid(
                    "R-RECEIPT-EVIDENCE-FORM",
                    &format!("{at}.rule"),
                    format!(
                        "rule must be the literal {DEFAULT_RULE:?}, got {:?}",
                        e.rule
                    ),
                ));
            }
            check_digest_at(&e.value_digest, &format!("{at}.value_digest"))?;
            if i > 0 {
                check_ascending(
                    &self.applied_defaults[i - 1].path,
                    &e.path,
                    &format!("{at}.path"),
                )?;
            }
        }
        for (i, e) in self.derived_values.iter().enumerate() {
            let at = format!("{}[{i}]", field_path("derived_values"));
            check_path_form(&e.path, &format!("{at}.path"))?;
            check_derived_rule(&e.rule, &format!("{at}.rule"))?;
            for (j, input) in e.inputs.iter().enumerate() {
                let iat = format!("{at}.inputs[{j}]");
                check_path_form(input, &iat)?;
                if j > 0 {
                    check_ascending(&e.inputs[j - 1], input, &iat)?;
                }
            }
            check_digest_at(&e.value_digest, &format!("{at}.value_digest"))?;
            if i > 0 {
                check_ascending(
                    &self.derived_values[i - 1].path,
                    &e.path,
                    &format!("{at}.path"),
                )?;
            }
        }
        Ok(())
    }
}

fn parse_default(v: &Value, at: &str) -> Result<DefaultEvidence> {
    let m = take_map(v, at)?;
    let out = DefaultEvidence {
        path: take_str(m, "path", &format!("{at}.path"))?,
        rule: take_str(m, "rule", &format!("{at}.rule"))?,
        value_digest: take_str(m, "value_digest", &format!("{at}.value_digest"))?,
    };
    check_keys(m, &["path", "rule", "value_digest"], at)?;
    Ok(out)
}

fn parse_derived(v: &Value, at: &str) -> Result<DerivedEvidence> {
    let m = take_map(v, at)?;
    let path = take_str(m, "path", &format!("{at}.path"))?;
    let rule = take_str(m, "rule", &format!("{at}.rule"))?;
    let inputs_at = format!("{at}.inputs");
    let inputs = match m.get("inputs") {
        Some(Value::Seq(items)) => {
            let mut out = Vec::with_capacity(items.len());
            for (j, item) in items.iter().enumerate() {
                match item {
                    Value::Str(s) => out.push(s.clone()),
                    _ => return Err(shape(&format!("{inputs_at}[{j}]"), "a string")),
                }
            }
            out
        }
        Some(_) => return Err(shape(&inputs_at, "a sequence")),
        None => return Err(shape(&inputs_at, "this required field (it is absent)")),
    };
    let value_digest = take_str(m, "value_digest", &format!("{at}.value_digest"))?;
    check_keys(m, &["path", "rule", "inputs", "value_digest"], at)?;
    Ok(DerivedEvidence {
        path,
        rule,
        inputs,
        value_digest,
    })
}

fn is_digest(s: &str) -> bool {
    s.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    })
}

fn check_digest_at(s: &str, path: &str) -> Result<()> {
    if is_digest(s) {
        Ok(())
    } else {
        Err(invalid(
            "R-RECEIPT-DIGEST-FORMAT",
            path,
            "a digest is `sha256:` followed by 64 lowercase hex characters".to_string(),
        ))
    }
}

fn check_digest(s: &str, name: &str) -> Result<()> {
    check_digest_at(s, &field_path(name))
}

fn check_nonempty(s: &str, name: &str) -> Result<()> {
    if s.is_empty() {
        Err(shape(&field_path(name), "a non-empty string"))
    } else {
        Ok(())
    }
}

/// `$.` followed by one or more non-empty dot-separated segments.
fn check_path_form(p: &str, at: &str) -> Result<()> {
    let ok = p
        .strip_prefix("$.")
        .is_some_and(|rest| rest.split('.').all(|seg| !seg.is_empty()));
    if ok {
        Ok(())
    } else {
        Err(invalid(
            "R-RECEIPT-EVIDENCE-FORM",
            at,
            format!("{p:?} is not a `$.`-rooted path with non-empty segments"),
        ))
    }
}

/// `name` + `-v` + one or more ASCII digits, with a non-empty name.
fn check_derived_rule(rule: &str, at: &str) -> Result<()> {
    let ok = rule.rsplit_once("-v").is_some_and(|(name, n)| {
        !name.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
    });
    if ok {
        Ok(())
    } else {
        Err(invalid(
            "R-RECEIPT-EVIDENCE-FORM",
            at,
            format!("{rule:?} is not a versioned rule identifier (`name-vN`)"),
        ))
    }
}

/// Strictly ascending in raw UTF-8 byte order: equal means a duplicate,
/// smaller means out of order.
fn check_ascending(prev: &str, cur: &str, at: &str) -> Result<()> {
    if prev < cur {
        return Ok(());
    }
    let what = if prev == cur {
        "duplicate"
    } else {
        "out of order"
    };
    Err(invalid(
        "R-RECEIPT-ORDER",
        at,
        format!("{cur:?} is {what} after {prev:?} (strictly ascending byte order required)"),
    ))
}

#[derive(PartialEq, Eq)]
enum Kind {
    Default,
    Derived,
    Other,
}

fn kind(f: &MaterializedField) -> Kind {
    match &f.evidence {
        FieldEvidence::Intent(IntentEvidence::SchemaDefault(_)) => Kind::Default,
        FieldEvidence::Intent(IntentEvidence::Derived(_))
        | FieldEvidence::DerivedObservation(_) => Kind::Derived,
        _ => Kind::Other,
    }
}

fn unbound(at: &str, path: &str) -> Error {
    Error::new(
        code::RECEIPT_UNBOUND,
        "R-RECEIPT-PATH-BINDING",
        Stage::Validate,
        at,
        format!("receipt path {path:?} has no explicit binding to a materialized object key"),
    )
}

/// Resolves one evidence entry to its bound field and compares the value
/// digest. `want` is the kind the entry's list requires.
fn check_entry<'a>(
    object: &MaterializedObject,
    binding: &'a PathBinding,
    path: &str,
    value_digest: &str,
    want: &Kind,
    at: &str,
) -> Result<&'a str> {
    let Some(key) = binding.get(path) else {
        return Err(unbound(&format!("{at}.path"), path));
    };
    let Some(field) = object.get(key) else {
        return Err(mismatch(
            "R-RECEIPT-EVIDENCE-MEMBERSHIP",
            &format!("{at}.path"),
            format!("{path:?} is bound to {key:?}, which is not a key of the object"),
        ));
    };
    if &kind(field) != want {
        return Err(mismatch(
            "R-RECEIPT-EVIDENCE-MEMBERSHIP",
            &format!("{at}.path"),
            format!("object field {key:?} does not have the provenance this list requires"),
        ));
    }
    let Some(value) = field.value() else {
        return Err(mismatch(
            "R-RECEIPT-EVIDENCE-VALUE",
            &format!("{at}.value_digest"),
            format!("object field {key:?} carries no value to digest"),
        ));
    };
    let got = digest(&to_canonical_json(value)?);
    if got != value_digest {
        return Err(mismatch(
            "R-RECEIPT-EVIDENCE-VALUE",
            &format!("{at}.value_digest"),
            format!("value digest of {key:?} is {got}, the receipt says {value_digest}"),
        ));
    }
    Ok(key)
}

/// Verifies `receipt` against the object it describes and the authored input
/// it was made from. See the module documentation for exactly what a success
/// does and does not establish.
///
/// `binding` maps receipt paths to `object` keys; it may be empty only when
/// the receipt lists no evidence.
///
/// # Errors
/// * `E_INVALID_RECEIPT` -- shape, version, digest form, evidence form or order;
/// * `E_RECEIPT_MISMATCH` -- a digest or an evidence entry disagrees with the
///   object or the input;
/// * `E_RECEIPT_UNBOUND` -- an evidence entry has no explicit path binding.
pub fn verify_receipt(
    receipt: &Receipt,
    object: &MaterializedObject,
    authored_input: &Value,
    binding: &PathBinding,
) -> Result<()> {
    receipt.check_structure()?;

    let input = digest(&to_canonical_json(authored_input)?);
    if input != receipt.input_digest {
        return Err(mismatch(
            "R-RECEIPT-BINDS-INPUT",
            &field_path("input_digest"),
            format!("canonical digest of the authored input is {input}"),
        ));
    }
    let output = materialized_object_digest(object)?;
    if output != receipt.output_digest {
        return Err(mismatch(
            "R-RECEIPT-BINDS-OUTPUT",
            &field_path("output_digest"),
            format!("digest of the materialized object is {output}"),
        ));
    }

    let mut defaulted: BTreeSet<&str> = BTreeSet::new();
    for (i, e) in receipt.applied_defaults.iter().enumerate() {
        let at = format!("{}[{i}]", field_path("applied_defaults"));
        let key = check_entry(
            object,
            binding,
            &e.path,
            &e.value_digest,
            &Kind::Default,
            &at,
        )?;
        if !defaulted.insert(key) {
            return Err(mismatch(
                "R-RECEIPT-EVIDENCE-MEMBERSHIP",
                &format!("{at}.path"),
                format!("object field {key:?} is listed twice"),
            ));
        }
    }
    let mut derived: BTreeSet<&str> = BTreeSet::new();
    for (i, e) in receipt.derived_values.iter().enumerate() {
        let at = format!("{}[{i}]", field_path("derived_values"));
        let key = check_entry(
            object,
            binding,
            &e.path,
            &e.value_digest,
            &Kind::Derived,
            &at,
        )?;
        if !derived.insert(key) {
            return Err(mismatch(
                "R-RECEIPT-EVIDENCE-MEMBERSHIP",
                &format!("{at}.path"),
                format!("object field {key:?} is listed twice"),
            ));
        }
    }

    for (key, field) in object.iter() {
        let (listed, list) = match kind(field) {
            Kind::Default => (defaulted.contains(key.as_str()), "applied_defaults"),
            Kind::Derived => (derived.contains(key.as_str()), "derived_values"),
            Kind::Other => continue,
        };
        if !listed {
            return Err(mismatch(
                "R-RECEIPT-EVIDENCE-MEMBERSHIP",
                &format!("object/{key}"),
                format!("the object's field {key:?} has no entry in {list}"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materialized::Capability;
    use crate::reader::Map;

    const D: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    fn field(evidence: FieldEvidence) -> MaterializedField {
        MaterializedField {
            capability: Capability::Implemented,
            evidence,
        }
    }

    fn object(fields: Vec<(&str, MaterializedField)>) -> MaterializedObject {
        let keys = fields.iter().map(|(k, _)| (*k).to_string()).collect();
        let map = fields
            .into_iter()
            .map(|(k, f)| (k.to_string(), f))
            .collect();
        MaterializedObject::try_new(map, &keys).expect("fixture keys match themselves")
    }

    fn input() -> Value {
        let mut m = Map::default();
        m.push("shape", Value::Str("E4.Flex".into()));
        Value::Map(m)
    }

    fn empty_receipt(obj: &MaterializedObject) -> Receipt {
        Receipt {
            receipt_schema_version: 1,
            grammar_sha256: D.into(),
            grammar_schema_sha256: D.into(),
            primitive_release: "primitives/@v0.2.0".into(),
            primitive_source_commit: "960ee0f".into(),
            primitive_build_hash: D.into(),
            schema_name: "cic:storage:StorageResource".into(),
            schema_version: "v1.2.0".into(),
            schema_digest: D.into(),
            engine_identity: "environment-under-test".into(),
            engine_version: "0.0.0".into(),
            input_digest: digest(&to_canonical_json(&input()).unwrap()),
            output_digest: materialized_object_digest(obj).unwrap(),
            applied_defaults: vec![],
            derived_values: vec![],
        }
    }

    fn authored_only() -> MaterializedObject {
        object(vec![(
            "shape",
            field(FieldEvidence::Intent(IntentEvidence::Authored(Some(
                Value::Str("E4.Flex".into()),
            )))),
        )])
    }

    #[test]
    fn empty_evidence_over_an_object_without_defaults_verifies_without_a_binding() {
        let obj = authored_only();
        let r = empty_receipt(&obj);
        verify_receipt(&r, &obj, &input(), &PathBinding::new()).expect("must verify");
    }

    #[test]
    fn a_defaulted_field_the_receipt_does_not_list_is_refused() {
        let obj = object(vec![(
            "replicas",
            field(FieldEvidence::Intent(IntentEvidence::SchemaDefault(
                Value::Int(3),
            ))),
        )]);
        let r = empty_receipt(&obj);
        let e = verify_receipt(&r, &obj, &input(), &PathBinding::new()).unwrap_err();
        assert_eq!(e.code, code::RECEIPT_MISMATCH);
        assert_eq!(e.path, "object/replicas");
    }

    #[test]
    fn evidence_without_a_binding_fails_closed() {
        let obj = object(vec![(
            "replicas",
            field(FieldEvidence::Intent(IntentEvidence::SchemaDefault(
                Value::Int(3),
            ))),
        )]);
        let mut r = empty_receipt(&obj);
        r.applied_defaults.push(DefaultEvidence {
            path: "$.config.replicas".into(),
            rule: DEFAULT_RULE.into(),
            value_digest: digest(&to_canonical_json(&Value::Int(3)).unwrap()),
        });
        let e = verify_receipt(&r, &obj, &input(), &PathBinding::new()).unwrap_err();
        assert_eq!(e.code, code::RECEIPT_UNBOUND);

        let mut b = PathBinding::new();
        b.insert("$.config.replicas".into(), "replicas".into());
        verify_receipt(&r, &obj, &input(), &b).expect("bound, consistent");
    }

    #[test]
    fn derived_inputs_are_not_required_to_exist_in_the_object() {
        let obj = object(vec![(
            "effective_state",
            field(FieldEvidence::Intent(IntentEvidence::Derived(Value::Str(
                "RUNNING".into(),
            )))),
        )]);
        let mut r = empty_receipt(&obj);
        r.derived_values.push(DerivedEvidence {
            path: "$.state.effective_state".into(),
            rule: "effective-state-v1".into(),
            inputs: vec!["$.state.admin_state".into(), "$.state.oper_state".into()],
            value_digest: digest(&to_canonical_json(&Value::Str("RUNNING".into())).unwrap()),
        });
        let mut b = PathBinding::new();
        b.insert("$.state.effective_state".into(), "effective_state".into());
        verify_receipt(&r, &obj, &input(), &b).expect("inputs are checked for form and order only");
    }

    #[test]
    fn digest_form_is_exactly_sha256_and_64_lowercase_hex() {
        assert!(is_digest(D));
        assert!(!is_digest(&D.to_uppercase()));
        assert!(!is_digest(&D[7..]));
        assert!(!is_digest("sha256:00"));
        assert!(!is_digest(&format!("{D}0")));
        assert!(!is_digest(&format!("sha512:{}", &D[7..])));
    }

    #[test]
    fn path_and_rule_forms() {
        assert!(check_path_form("$.a.b", "x").is_ok());
        for bad in ["$", "$.", "a.b", "$.a..b", "$.a."] {
            assert!(check_path_form(bad, "x").is_err(), "{bad}");
        }
        assert!(check_derived_rule("effective-state-v1", "x").is_ok());
        for bad in ["effective-state", "-v1", "x-v", "x-v1a", "x-vv1"] {
            assert!(check_derived_rule(bad, "x").is_err(), "{bad}");
        }
    }
}
