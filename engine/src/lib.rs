//! The CIC primitive engine.
//!
//! # The contract
//!
//! **A module does not interpret CIC schema. This engine interprets it,
//! materializes it, and proves it; the module works only on the closed data
//! set.**
//!
//! That sentence is the reason this crate exists, and it is a stronger
//! constraint than any directory layout. Everything here either serves it or
//! does not belong here.
//!
//! # The pipeline
//!
//! ```text
//! YAML bytes
//!   -> Stage::Read         strict document reading, before a tree exists
//!   -> Stage::Parse        typed composition; structural positions are closed
//!   -> Stage::Normalize    short forms expanded, applicable defaults applied
//!   -> Stage::Resolve      references, inheritance chains, cycle detection
//!   -> Stage::Validate     Shape algebra, Role algebra, cross-primitive rules
//!   -> Stage::Canonicalize one byte representation, for digests and caches
//! ```
//!
//! The stages are total and ordered: every rejection belongs to exactly one,
//! and a stage may only reject what the stages before it admitted. A field that
//! is missing cannot be "skipped" by a later stage, because the position it
//! would occupy is fixed at `Parse` and does not depend on the field being
//! there. That is the failure this design exists to prevent: a checker that
//! discovers work by looking for a member is blind exactly when the member is
//! absent, and reports zero findings rather than one.
//!
//! # What does not belong in this crate
//!
//! Vault access, counter-signature policy, git and release workflow, domain
//! adapters, Kubernetes/OCI/network runtime logic, and authorization decisions.
//! A release verifier may call this engine to check the specs it carries, but
//! trust chain and provenance stay outside. Whether a primitive is semantically
//! valid and whether it came from someone you trust are different questions,
//! and merging them is how the archived `cic-object-model` grew into a
//! repository that had to be abandoned.
//!
//! # Status
//!
//! Early. `error` and `reader` are extracted and working; the stages above
//! `Read` are not implemented yet. Nothing here is a stable API, and no module
//! should depend on it as one.

pub mod error;
pub mod reader;

pub use error::{code, Error, Result, Stage};
