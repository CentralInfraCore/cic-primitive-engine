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
    //! but the two below named parts of its ontology — `E_ORIGIN_*`,
    //! `E_SEALED_*`, `E_UNKNOWN_PRIMITIVE`. They are deliberately not carried
    //! over: an error code is a promise about what the engine can be asked to
    //! reject, and promising the vocabulary of a rejected model would reinstate
    //! it in the one file measured as free of it.
    //!
    //! Codes are added as the stage that raises them is implemented, never in
    //! advance. A code with no raiser and no vector is a claim, not a check.

    /// The bytes are not a document this engine will read at all: bad encoding,
    /// a duplicate mapping key, an alias, a non-string key, more than one
    /// document. Raised at `Stage::Read`, before any tree exists.
    pub const MALFORMED_DOCUMENT: &str = "E_MALFORMED_DOCUMENT";

    /// A structural position holds something of the wrong arity — a mapping
    /// where the grammar fixes a list, or the reverse. Raised at `Stage::Parse`.
    pub const TYPE_MISMATCH: &str = "E_TYPE_MISMATCH";
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
