//! Runs the language-independent corpus in `conformance/`.
//!
//! The harness enforces two properties about ITSELF, not just about the engine.
//! Both exist because a gate that cannot fail is not a gate, and both failure
//! modes are easy to reach by accident:
//!
//! * a corpus that lost its files still reports success, because zero
//!   assertions all pass;
//! * a checker that rejects every input satisfies a corpus made only of
//!   rejections.
//!
//! So: an empty corpus fails, and a group without at least one accepted vector
//! fails.

use std::fs;
use std::path::{Path, PathBuf};

use cic_primitive_engine::{digest, reader, to_canonical_json, Stage};

/// The minimum a group must contain before it is allowed to report success.
const MIN_VECTORS_PER_GROUP: usize = 2;

struct Vector {
    name: String,
    input: Vec<u8>,
    accepted: bool,
    code: Option<String>,
    stage: Option<String>,
    /// `canonicalize/`'s own extension to the shared `expected.yaml`
    /// vocabulary: the exact canonical JSON bytes an accepted vector must
    /// produce. Harmless, always `None`, for groups that don't declare it
    /// (`reader/`).
    canonical: Option<String>,
    /// Same extension, for the rarer vectors that also pin A7's digest —
    /// most accepted vectors only need to pin `canonical`, since the
    /// digest is a pure function of it, already covered by `canonical.rs`'s
    /// own unit tests.
    digest: Option<String>,
}

/// Strips one layer of single-quoting, if present on both ends — the
/// `canonical`/`digest` fields are always single-quoted in `expected.yaml`
/// so a value starting with `{`/`[` can't be misread as YAML flow syntax by
/// a real YAML parser that later consumes this same corpus.
fn unquote(s: &str) -> String {
    s.strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(s)
        .to_string()
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("engine/ has a parent")
        .join("conformance")
}

/// Deliberately hand-rolled: the corpus must be readable without depending on
/// the engine's own reader, or a reader bug could hide the vectors that catch it.
fn scalar_field(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim_end();
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            return Some(rest.trim().to_string());
        }
    }
    None
}

fn load_group(group: &Path) -> Vec<Vector> {
    // `canonicalize/`'s own structural invariant, enforced here rather than
    // left as a runtime-optional convention (review-caught on PR #21): an
    // accepted vector in this specific group must declare `canonical`, or
    // it would pass having checked zero canonical bytes -- exactly the
    // "gate that cannot fail" risk this corpus's own two generic rules
    // already guard against, just one level more specific.
    let group_name = group.file_name().map(|n| n.to_string_lossy().to_string());
    let requires_canonical = group_name.as_deref() == Some("canonicalize");

    let mut vectors = Vec::new();
    let entries = fs::read_dir(group).unwrap_or_else(|e| panic!("{}: {e}", group.display()));
    for entry in entries {
        let dir = entry.expect("readable entry").path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().to_string();
        let input =
            fs::read(dir.join("input.yaml")).unwrap_or_else(|e| panic!("{name}: input.yaml: {e}"));
        let expected = fs::read_to_string(dir.join("expected.yaml"))
            .unwrap_or_else(|e| panic!("{name}: expected.yaml: {e}"));
        let outcome = scalar_field(&expected, "outcome")
            .unwrap_or_else(|| panic!("{name}: expected.yaml has no `outcome`"));
        let accepted = match outcome.as_str() {
            "accepted" => true,
            "rejected" => false,
            other => panic!("{name}: outcome must be accepted or rejected, found `{other}`"),
        };
        // Both required for every rejection, in every group, matching what
        // this corpus's own README already states -- previously only
        // `code` was actually enforced (review-caught on PR #21); every
        // existing rejection vector already declares both, so this
        // tightens the check without changing any vector.
        if !accepted {
            if scalar_field(&expected, "code").is_none() {
                panic!("{name}: a rejection vector must state the code it expects");
            }
            if scalar_field(&expected, "stage").is_none() {
                panic!("{name}: a rejection vector must state the stage it expects");
            }
        }
        let canonical = scalar_field(&expected, "canonical").map(|s| unquote(&s));
        if requires_canonical && accepted && canonical.is_none() {
            panic!(
                "{name}: an accepted vector in `canonicalize/` must state `canonical` \
                 — otherwise it passes having checked zero canonical bytes"
            );
        }
        vectors.push(Vector {
            name,
            input,
            accepted,
            code: scalar_field(&expected, "code"),
            stage: scalar_field(&expected, "stage"),
            canonical,
            digest: scalar_field(&expected, "digest").map(|s| unquote(&s)),
        });
    }
    vectors.sort_by(|a, b| a.name.cmp(&b.name));
    vectors
}

#[test]
fn the_corpus_is_not_empty() {
    let root = corpus_root();
    let groups: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        // `differential/` is a directory of GROUPS (comparator/,
        // collection/, ...), each holding `input.json`/`expected.json`
        // vectors with no accepted/rejected outcome at all -- a
        // structurally different corpus type (see its own README), with
        // its own dedicated test (tests/differential.rs) enforcing its
        // own non-empty/full-coverage invariants. Scanning it here as if
        // it held `input.yaml`/`expected.yaml` vectors directly would
        // fail on a directory shape this generic check was never meant
        // to understand, not catch a real gap.
        .filter(|p| p.file_name().and_then(|n| n.to_str()) != Some("differential"))
        .collect();
    assert!(
        !groups.is_empty(),
        "the conformance corpus has no groups: every run would pass by asserting nothing"
    );
    for group in groups {
        let vectors = load_group(&group);
        assert!(
            vectors.len() >= MIN_VECTORS_PER_GROUP,
            "{}: {} vector(s), at least {MIN_VECTORS_PER_GROUP} required",
            group.display(),
            vectors.len()
        );
        assert!(
            vectors.iter().any(|v| v.accepted),
            "{}: every vector is a rejection — a checker that refuses all input \
             would pass this group",
            group.display()
        );
    }
}

#[test]
fn reader_vectors() {
    let group = corpus_root().join("reader");
    let vectors = load_group(&group);
    let mut failures = Vec::new();

    for v in &vectors {
        let result = reader::parse(&v.input, Stage::Read, "$", "document");
        match (&result, v.accepted) {
            (Ok(_), true) => {}
            (Err(e), false) => {
                if let Some(expected) = &v.code {
                    if e.code != *expected {
                        failures.push(format!(
                            "{}: expected code {expected}, got {}",
                            v.name, e.code
                        ));
                    }
                }
                if let Some(expected) = &v.stage {
                    if e.stage.as_str() != expected {
                        failures.push(format!(
                            "{}: expected stage {expected}, got {}",
                            v.name,
                            e.stage.as_str()
                        ));
                    }
                }
            }
            (Ok(_), false) => failures.push(format!(
                "{}: LEAKED — expected a rejection, the document was accepted",
                v.name
            )),
            (Err(e), true) => failures.push(format!(
                "{}: expected acceptance, rejected with {}",
                v.name, e.code
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} reader vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );
}

/// `conformance/canonicalize/` — section A's own vectors (G1's own, already
/// named follow-up: "adding a corpus group only gets the generic invariant
/// checks; the hand-written test that actually runs [it] still has to be
/// written"). Each document is read with `reader::parse`, then
/// canonicalized; an accepted vector's exact canonical bytes (and,
/// sometimes, A7's digest) are pinned, not just "did it not error" —
/// language-independent by construction, so a future Go peer can consume
/// the identical fixtures.
#[test]
fn canonicalize_vectors() {
    let group = corpus_root().join("canonicalize");
    let vectors = load_group(&group);
    let mut failures = Vec::new();

    for v in &vectors {
        let value = match reader::parse(&v.input, Stage::Read, "$", "document") {
            Ok(value) => value,
            Err(e) => {
                failures.push(format!("{}: failed to parse as a document: {e}", v.name));
                continue;
            }
        };
        let result = to_canonical_json(&value);
        match (&result, v.accepted) {
            (Ok(bytes), true) => {
                let got = String::from_utf8(bytes.clone())
                    .unwrap_or_else(|e| panic!("{}: canonical output is not UTF-8: {e}", v.name));
                if let Some(want) = &v.canonical {
                    if &got != want {
                        failures.push(format!(
                            "{}: canonical mismatch\n    got:  {got}\n    want: {want}",
                            v.name
                        ));
                    }
                }
                if let Some(want_digest) = &v.digest {
                    let got_digest = digest(bytes);
                    if &got_digest != want_digest {
                        failures.push(format!(
                            "{}: digest mismatch\n    got:  {got_digest}\n    want: {want_digest}",
                            v.name
                        ));
                    }
                }
            }
            (Err(e), false) => {
                if let Some(expected) = &v.code {
                    if e.code != *expected {
                        failures.push(format!(
                            "{}: expected code {expected}, got {}",
                            v.name, e.code
                        ));
                    }
                }
                if let Some(expected) = &v.stage {
                    if e.stage.as_str() != expected {
                        failures.push(format!(
                            "{}: expected stage {expected}, got {}",
                            v.name,
                            e.stage.as_str()
                        ));
                    }
                }
            }
            (Ok(_), false) => failures.push(format!(
                "{}: LEAKED — expected canonicalization to reject this, it produced bytes",
                v.name
            )),
            (Err(e), true) => failures.push(format!(
                "{}: expected acceptance, canonicalization rejected with {}",
                v.name, e.code
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} canonicalize vectors failed:\n  {}",
        failures.len(),
        vectors.len(),
        failures.join("\n  ")
    );
}
