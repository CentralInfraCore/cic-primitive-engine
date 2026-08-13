//! Strict document reading.
//!
//! # Provenance
//!
//! Extracted from `cic-object-model` (archived, rejected direction) as
//! `rust/src/value.rs`, the one file measured at zero model-specific
//! references. What changed on the way in: the rule identifiers. The original
//! raised `INV-013`, `INV-041`, `INV-042` and `INV-043` — invariants of a
//! specification that no longer holds. A rejection that cites a retired
//! invariant is a rejection nobody can look up, so they became this engine's
//! own `R-READ-*` rules. The behaviour is unchanged and the corpus proves it.
//!
//! An order-preserving YAML value.
//!
//! Why not use the parser's own value type directly: canonical output is
//! member-ordered, so insertion order has to survive parsing, and mapping keys
//! are always strings. A small owned type makes
//! both properties structural instead of something every call site remembers.
//!
//! `Map` is a `Vec` of pairs rather than a hash map. The corpus never has
//! mappings large enough for lookup cost to matter, and a `Vec` keeps order
//! without a second dependency.
//!
//! # Duplicate keys are refused, in the same pass
//!
//! `a: 1` followed by `a: 2` in one mapping does not reach the tree builder as
//! two entries: the composer resolves it to `{a: 2}`, last wins, silently. A
//! check after composition would therefore be unreachable code claiming a
//! guarantee it cannot make.
//!
//! It was left at that for one commit, on the reasoning that the Go
//! implementation behaved the same way and the specification said nothing, so
//! the two at least agreed. **That reasoning was wrong and it was never
//! measured.** Go rejects the document outright — `mapping key "a" already
//! defined at line 1` — which means the two implementations disagreed on real
//! input, in a structure whose stated purpose is unique addressing, and
//! the corpus could not see it because no vector contains a duplicate.
//!
//! Finding that is what a second implementation is FOR. Go's behaviour is the
//! right one, so the scan below refuses duplicates too, at the event level
//! where both keys are still visible.
//!
//! # Anchors and aliases are refused, before a tree exists
//!
//! An alias is not part of the schema language or the authoring format, and
//! leaving it to the tree builder is not an option. Measured on this parser:
//!
//! ```text
//! 393 bytes of nested aliases -> 12,345,678 nodes, 2.7 seconds
//! ```
//!
//! That is a ~31,000x amplification on input this library treats as untrusted,
//! and it happens during COMPOSITION — by the time a value reaches the code
//! below, the memory is already spent, so no budget checked here can help.
//!
//! So the document is scanned as an event stream first. Scanning is linear in
//! the input's own size and never expands an alias, so an `Alias` event can be
//! refused for the price of reading the bytes once.

use crate::error::{code, Error, Result, Stage};
use saphyr::{LoadableYamlNode, Yaml};
use saphyr_parser::{Event, Parser};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Seq(Vec<Value>),
    Map(Map),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Map(pub Vec<(String, Value)>);

impl Map {
    #[must_use]
    pub fn get(&self, k: &str) -> Option<&Value> {
        self.0.iter().find(|(key, _)| key == k).map(|(_, v)| v)
    }
    #[must_use]
    pub fn contains(&self, k: &str) -> bool {
        self.get(k).is_some()
    }
    /// Append without looking for an existing key.
    ///
    /// `insert` searches the whole vector on every call, so building an n-key
    /// mapping with it costs `n(n-1)/2` comparisons — the second quadratic in
    /// this file, and the one that remained after the duplicate-key scan
    /// stopped being the first. The parser can append safely because
    /// `scan_input` has already refused duplicates, so there is nothing to
    /// overwrite.
    pub fn push(&mut self, k: impl Into<String>, v: Value) {
        self.0.push((k.into(), v));
    }

    pub fn insert(&mut self, k: impl Into<String>, v: Value) {
        let k = k.into();
        if let Some(slot) = self.0.iter_mut().find(|(key, _)| *key == k) {
            slot.1 = v;
        } else {
            self.0.push((k, v));
        }
    }
    #[must_use]
    pub fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(k, _)| k.as_str()).collect()
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

impl Value {
    #[must_use]
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Value::Map(m) => Some(m),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_seq(&self) -> Option<&[Value]> {
        match self {
            Value::Seq(s) => Some(s),
            _ => None,
        }
    }
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    /// The scalar rendering used in messages and in `shape.type`.
    #[must_use]
    pub fn to_plain_string(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Float(f) => f.to_string(),
            Value::Str(s) => s.clone(),
            Value::Seq(_) => "<sequence>".into(),
            Value::Map(_) => "<mapping>".into(),
        }
    }
}

/// Parse a YAML document.
///
/// `stage` and `path` are carried in because the same malformed-input failure
/// is raised from three different stages and each has to name its own.
///
/// # Errors
/// Returns `E_MALFORMED_DOCUMENT` when the bytes are not UTF-8, not YAML, use a
/// construct this model does not read, or carry a non-string mapping key.
pub fn parse(data: &[u8], stage: Stage, path: &str, what: &str) -> Result<Value> {
    let text = std::str::from_utf8(data).map_err(|e| {
        Error::new(
            code::MALFORMED_DOCUMENT,
            "R-READ-WELL-FORMED",
            stage,
            path,
            format!("{what} is not valid UTF-8: {e}"),
        )
    })?;

    scan_input(text, stage, path, what)?;

    let docs = Yaml::load_from_str(text).map_err(|e| {
        Error::new(
            code::MALFORMED_DOCUMENT,
            "R-READ-WELL-FORMED",
            stage,
            path,
            format!("{what} is not valid YAML: {e}"),
        )
    })?;

    // One document per composition. A second one used to be dropped
    // silently by taking `.next()`, so validation authenticated a PREFIX of the
    // supplied bytes: `-validate` printed `valid` for a file whose second
    // document was never looked at, while a consumer reading the same bytes
    // with a multi-document loader saw both. That is a parser-differential, and
    // the half that was checked is not the half a reader might act on.
    if docs.len() > 1 {
        return Err(Error::new(
            code::MALFORMED_DOCUMENT,
            "R-READ-SINGLE-DOCUMENT",
            stage,
            path,
            format!(
                "{what} contains {} YAML documents; an object is exactly one",
                docs.len()
            ),
        ));
    }

    // An empty document is the empty mapping. `{}`, an empty file and a file
    // holding only a `---` marker are the same input as far as this model is
    // concerned, and the corpus contains more than one of them. A bare `---`
    // parses as null rather than as no document at all, so both are folded here.
    let Some(doc) = docs.into_iter().next() else {
        return Ok(Value::Map(Map::default()));
    };
    if matches!(doc, Yaml::Value(saphyr::Scalar::Null)) {
        return Ok(Value::Map(Map::default()));
    }
    convert(&doc, stage, path, what)
}

/// Scan the document as an event stream and refuse what the composer would
/// otherwise silently absorb.
///
/// Two things are only visible here. An **alias**, because composing one is
/// where a few hundred bytes become millions of nodes — by the time a value
/// reaches the tree builder the memory is already spent. And a **duplicate
/// key**, because the composer resolves it to one entry before anything
/// downstream can count them.
///
/// Scanning is linear in the input's own size and expands nothing, so both cost
/// one read of the bytes.
fn scan_input(text: &str, stage: Stage, path: &str, what: &str) -> Result<()> {
    // The invariant is the rule the document broke, not the stage's generic
    // one: INV-041 for a duplicate key, INV-042 for an alias, INV-013 only for
    // a document the parser could not read at all.
    let refuse = |invariant: &'static str, detail: String| {
        Error::new(code::MALFORMED_DOCUMENT, invariant, stage, path, detail)
    };

    let mut stack: Vec<Frame> = Vec::new();

    for event in Parser::new_from_str(text) {
        let (event, _) = event.map_err(|e| {
            refuse(
                "R-READ-WELL-FORMED",
                format!("{what} is not valid YAML: {e}"),
            )
        })?;
        match event {
            Event::Alias(_) => {
                return Err(refuse(
                    "R-READ-ALIAS",
                    format!(
                        "{what} uses a YAML anchor or alias; neither is part of the \
                         format, and expanding an alias can turn a few hundred bytes \
                         into millions of nodes"
                    ),
                ))
            }
            Event::MappingStart(..) => {
                consume_value(&mut stack);
                stack.push(Frame {
                    keys: HashSet::new(),
                    expecting_key: true,
                    is_mapping: true,
                });
            }
            Event::SequenceStart(..) => {
                consume_value(&mut stack);
                stack.push(Frame {
                    keys: HashSet::new(),
                    expecting_key: false,
                    is_mapping: false,
                });
            }
            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
            }
            Event::Scalar(value, ..) => {
                let key = match stack.last_mut() {
                    Some(f) if f.is_mapping && f.expecting_key => {
                        f.expecting_key = false;
                        Some(value.to_string())
                    }
                    Some(f) if f.is_mapping => {
                        f.expecting_key = true;
                        None
                    }
                    _ => None,
                };
                if let Some(k) = key {
                    let frame = stack.last_mut().expect("the frame is still open");
                    if frame.keys.contains(&k) {
                        return Err(refuse(
                            "R-READ-DUPLICATE-KEY",
                            format!(
                                "{what} declares `{k}` twice in one mapping; the same \
                                 name at different addresses is legal, one address \
                                 written twice is not"
                            ),
                        ));
                    }
                    frame.keys.insert(k);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// One frame per open collection: the keys seen so far, and whether the next
/// scalar in it is a key or a value. Sequences push a frame too, so a mapping
/// nested inside one does not inherit its parent's key set.
struct Frame {
    /// A set, not a `Vec`.
    ///
    /// This was a `Vec<String>` with a `contains` before every push, which is
    /// `n(n-1)/2` string comparisons for a flat mapping of n keys — quadratic,
    /// under a comment claiming the scan is linear in the input's own size.
    /// Measured on the `Vec`: 10k keys 1.0s, 20k 2.9s, 40k 11.6s for 389 KB.
    /// Cheap to write, expensive to read, which is the shape of every
    /// amplification finding in this file.
    keys: HashSet<String>,
    expecting_key: bool,
    is_mapping: bool,
}

/// A composite appearing where a mapping expects a value moves the frame on to
/// the next key.
fn consume_value(stack: &mut [Frame]) {
    if let Some(f) = stack.last_mut() {
        if f.is_mapping && !f.expecting_key {
            f.expecting_key = true;
        }
    }
}

fn convert(y: &Yaml, stage: Stage, path: &str, what: &str) -> Result<Value> {
    Ok(match y {
        Yaml::Value(scalar) => convert_scalar(scalar),
        Yaml::Sequence(items) => Value::Seq(
            items
                .iter()
                .map(|i| convert(i, stage, path, what))
                .collect::<Result<Vec<_>>>()?,
        ),
        Yaml::Mapping(m) => {
            let mut out = Map::default();
            for (k, v) in m {
                // Non-string keys are refused rather than stringified. YAML
                // allows `1:` and `true:`, and coercing them would let two
                // distinct keys collapse into one name — a silent collision in
                // a structure whose whole purpose is unique addressing.
                let Yaml::Value(saphyr::Scalar::String(key)) = k else {
                    return Err(Error::new(
                        code::MALFORMED_DOCUMENT,
                        "R-READ-KEY-TYPE",
                        stage,
                        path,
                        format!("{what} has a non-string mapping key; node names are strings"),
                    ));
                };
                out.push(key.to_string(), convert(v, stage, path, what)?);
            }
            Value::Map(out)
        }
        // Aliases and tagged nodes are not part of the schema language or the
        // authoring format. Refusing them keeps the input a tree.
        _ => {
            return Err(Error::new(
                code::MALFORMED_DOCUMENT,
                "R-READ-TREE-ONLY",
                stage,
                path,
                format!(
                "{what} uses a YAML construct this model does not read (alias, tag or bad value)"
            ),
            ))
        }
    })
}

fn convert_scalar(s: &saphyr::Scalar) -> Value {
    match s {
        saphyr::Scalar::Null => Value::Null,
        saphyr::Scalar::Boolean(b) => Value::Bool(*b),
        saphyr::Scalar::Integer(i) => Value::Int(*i),
        saphyr::Scalar::FloatingPoint(f) => Value::Float(f.into_inner()),
        saphyr::Scalar::String(s) => Value::Str(s.to_string()),
    }
}
