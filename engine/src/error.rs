//! Rejections.
//!
//! Every rejection carries the same four fields the conformance vectors assert
//! on — code, rule, stage, path — plus a human-readable detail that is
//! deliberately NOT compared by the corpus. The stage matters as much as the
//! code: the pipeline stages run in a fixed order, and a later stage cannot
//! observe input an earlier one would have rejected, so a rejection raised at
//! the wrong stage is a defect even when the code is right.
//!
//! # Provenance
//!
//! Adapted from `cic-object-model` (archived, rejected direction). The shape of
//! this type is what carried over; the STAGE NAMES did not. The original enum
//! spelled the stages of that model's materializer — `schema-load`,
//! `entry-validation`, `default-materialization`, `primitive-evaluation`,
//! `final-validation`. Those describe a pipeline this engine does not have.
//! Copying them would have imported the rejected ontology through the back
//! door, in the one file that was otherwise free of it.

use std::fmt;

/// The pipeline stage a rejection was raised at.
///
/// The order is the processing order, and it is total: every rejection belongs
/// to exactly one stage, and a stage may only reject what the stages before it
/// have already admitted.
///
/// `Parse`/`Normalize`/`Resolve`/`Validate` currently have no raiser in this
/// crate at all -- by design, not by omission: `lib.rs`'s "Division of
/// labor" scope correction names these as the environment's own job, not
/// this crate's. They stay in this enum as the taxonomy a rejection's
/// `stage` field is drawn from, the same way `Parse`/`Resolve`/`Validate`
/// already had no raiser before `Normalize` joined them here: `role.rs`'s
/// `expand_role` (Role short/long form, P0.2) raised `Normalize` until
/// 2026-10-07, when it was removed as out-of-scope, dead code (zero
/// internal callers) rather than kept to preserve a raiser this crate
/// never actually owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    /// Bytes to a raw document: encoding, duplicate keys, aliases, key types.
    Read,
    /// Raw document to a typed composition: structural positions and members.
    Parse,
    /// Short forms to canonical forms, and schema defaults to explicit values.
    Normalize,
    /// Atomic, aggregate and shape references; inheritance chains; cycles.
    Resolve,
    /// Shape algebra, Role algebra, and the contracts that cross primitives.
    Validate,
    /// Accepting a candidate as a genuine `MaterializedObject` --
    /// `PRIMITIVE-IR.md`'s Complete property and `FieldEvidence`'s own
    /// shape invariants. Added past the original five-stage pipeline this
    /// enum's doc comment once described in full: `lib.rs`'s "Division of
    /// labor" scope correction named this stage as an obligation this
    /// crate owes and had not yet built; `materialized.rs` is where it
    /// lives now. Unlike `Parse`/`Normalize`/`Resolve`/`Validate`, above,
    /// this one is not the environment's job.
    Materialize,
    /// The single byte representation an IR digest is taken over.
    Canonicalize,
}

impl Stage {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Stage::Read => "read",
            Stage::Parse => "parse",
            Stage::Normalize => "normalize",
            Stage::Resolve => "resolve",
            Stage::Validate => "validate",
            Stage::Materialize => "materialize",
            Stage::Canonicalize => "canonicalize",
        }
    }
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub mod code {
    //! Stable rejection codes.
    //!
    //! The archived `cic-object-model` shipped nineteen constants here, and all
    //! but one below named parts of its ontology — `E_ORIGIN_*`,
    //! `E_SEALED_*`, `E_UNKNOWN_PRIMITIVE`. They are deliberately not carried
    //! over: an error code is a promise about what the engine can be asked to
    //! reject, and promising the vocabulary of a rejected model would reinstate
    //! it in the one file measured as free of it.
    //!
    //! Codes are added as the stage that raises them is implemented, never in
    //! advance. A code with no raiser and no vector is a claim, not a check.
    //! `E_TYPE_MISMATCH` carried this rule's name over from the archived
    //! model's own `Stage::Parse` check (`materialize.rs`, a real raiser
    //! there) without carrying over a raiser here, violating the rule it
    //! sits next to from the crate's very first commit (`c9b46a5`,
    //! 2026-08-13) until removed on 2026-10-07 -- `Parse` was always the
    //! environment's stage, never this crate's, so no raiser was ever
    //! going to appear for it. Caught while checking `Stage::Normalize`'s
    //! own raiser status for an unrelated change (#43), named there, and
    //! closed here rather than left as a second instance of the same gap.

    /// The bytes are not a document this engine will read at all: bad encoding,
    /// a duplicate mapping key, an alias, a non-string key, more than one
    /// document. Raised at `Stage::Read`, before any tree exists.
    pub const MALFORMED_DOCUMENT: &str = "E_MALFORMED_DOCUMENT";

    /// A number is `NaN` or infinite. Canonical JSON (section A) has no
    /// representation for either — RFC 8259 numbers are finite by
    /// construction — so a value that reaches canonicalization still
    /// carrying one cannot be written, not "written specially". Raised at
    /// `Stage::Canonicalize`. Reachable today from authored YAML's own
    /// `.nan`/`.inf`/`-.inf` core-schema tags, verified empirically, not
    /// hypothetical.
    pub const NON_FINITE_NUMBER: &str = "E_NON_FINITE_NUMBER";

    /// A candidate's key set does not exactly match the key set the
    /// environment claims the schema declares -- missing a key, carrying
    /// an extra one, or both. `PRIMITIVE-IR.md`'s Complete property,
    /// enforced at the one point this crate can enforce it: construction.
    /// Raised at `Stage::Materialize`.
    pub const INCOMPLETE_OBJECT: &str = "E_INCOMPLETE_OBJECT";
}

/// The single error type this crate raises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub code: &'static str,
    pub rule: &'static str,
    pub stage: Stage,
    pub path: String,
    pub detail: String,
}

impl Error {
    pub(crate) fn new(
        code: &'static str,
        rule: &'static str,
        stage: Stage,
        path: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            rule,
            stage,
            path: path.into(),
            detail: detail.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}) at {} [stage {}]: {}",
            self.code, self.rule, self.path, self.stage, self.detail
        )
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
