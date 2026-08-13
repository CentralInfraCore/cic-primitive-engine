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

use cic_primitive_engine::{reader, Stage};

/// The minimum a group must contain before it is allowed to report success.
const MIN_VECTORS_PER_GROUP: usize = 2;

struct Vector {
    name: String,
    input: Vec<u8>,
    accepted: bool,
    code: Option<String>,
    stage: Option<String>,
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
        if !accepted && scalar_field(&expected, "code").is_none() {
            panic!("{name}: a rejection vector must state the code it expects");
        }
        vectors.push(Vector {
            name,
            input,
            accepted,
            code: scalar_field(&expected, "code"),
            stage: scalar_field(&expected, "stage"),
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
