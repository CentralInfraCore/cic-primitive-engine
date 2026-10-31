//! Domain schema identity (D1.3): the `schema_name` and the `schema_digest` of a receipt, and the binding of a claimed digest to the source bytes
//! (`docs/RECEIPT-SCHEMA.md`, "Schema identity contract"; `docs/MATERIALIZATION-SPEC.md`, D1.3). The Go peer is `go/schemaidentity`.
//!
//! This module defines what two receipt fields **mean**. It does not read a schema, authenticate where one came from, or verify a receipt:
//!
//! - `schema_digest` is the digest of the **raw bytes of the pinned schema source file**: `"sha256:"` and the lowercase hex of the SHA-256 of
//!   exactly the bytes the source authority published. Nothing is parsed, normalized or re-serialized first, so a comment, the key order, the
//!   line endings and a missing final newline are all part of the identity. It is the immutable identity of the *content*, not a claim that two
//!   differently written YAML documents are equal. It is not the section A digest of a value, though it is spelled the same way (A7).
//! - `schema_name` is `identity.namespace + ":" + identity.kind`, both read from the pinned schema's `spec.identity` and used as authored.
//!
//! The caller passes the identity components; this crate has no schema reader (`lib.rs`, "Division of labor"). A missing component is
//! `None`, an empty one is `Some("")`, and both are refused.
//!
//! The order of the checks is part of the contract (both languages must name the same first reason): the digest binding, then the namespace,
//! then the kind.

use crate::canonical;
use crate::error::{code, Error, Result, Stage};

/// The two identity fields of a receipt, as this contract defines them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaIdentity {
    pub schema_name: String,
    pub schema_digest: String,
}

/// `"sha256:"` and the lowercase hex SHA-256 of `source`, taken over the bytes as they are.
#[must_use]
pub fn schema_digest(source: &[u8]) -> String {
    canonical::digest(source)
}

/// `namespace + ":" + kind`, exactly as given, if both are well formed.
///
/// The namespace is non-empty and made of non-empty components separated by `:` (`cic:storage`); the kind is non-empty and holds no `:`, so the
/// last separator of the name is always the one this function put there. Nothing is trimmed, case-folded or escaped.
///
/// # Errors
/// `E_INVALID_SCHEMA_IDENTITY`, rule `R-SCHEMA-NAME-NAMESPACE` or `R-SCHEMA-NAME-KIND`.
pub fn schema_name(namespace: Option<&str>, kind: Option<&str>) -> Result<String> {
    let Some(namespace) = namespace else {
        return Err(invalid_identity(
            "R-SCHEMA-NAME-NAMESPACE",
            "$.spec.identity.namespace",
            "the namespace is absent",
        ));
    };
    if namespace.is_empty() || namespace.split(':').any(str::is_empty) {
        return Err(invalid_identity(
            "R-SCHEMA-NAME-NAMESPACE",
            "$.spec.identity.namespace",
            "the namespace is empty or has an empty component",
        ));
    }
    let Some(kind) = kind else {
        return Err(invalid_identity(
            "R-SCHEMA-NAME-KIND",
            "$.spec.identity.kind",
            "the kind is absent",
        ));
    };
    if kind.is_empty() || kind.contains(':') {
        return Err(invalid_identity(
            "R-SCHEMA-NAME-KIND",
            "$.spec.identity.kind",
            "the kind is empty or contains a colon",
        ));
    }
    Ok(format!("{namespace}:{kind}"))
}

/// Binds `claimed_digest` to the source bytes it must describe and returns the identity of the schema.
///
/// A claim that is absent, or that is not exactly the digest of `source`, is refused: a digest of other bytes (a preprocessed copy, a
/// re-serialized one) never stands in for the source. Consistency only: where `source` came from is not established here.
///
/// # Errors
/// `E_SCHEMA_SOURCE_BINDING` (rule `R-SCHEMA-DIGEST-MISSING` or `R-SCHEMA-DIGEST-MISMATCH`), else the errors of [`schema_name`].
pub fn bind_schema_identity(
    source: &[u8],
    claimed_digest: Option<&str>,
    namespace: Option<&str>,
    kind: Option<&str>,
) -> Result<SchemaIdentity> {
    let Some(claimed) = claimed_digest else {
        return Err(binding(
            "R-SCHEMA-DIGEST-MISSING",
            "no digest is claimed for the schema source",
        ));
    };
    let actual = schema_digest(source);
    if claimed != actual {
        return Err(binding(
            "R-SCHEMA-DIGEST-MISMATCH",
            "the claimed digest is not the digest of the source bytes",
        ));
    }
    Ok(SchemaIdentity {
        schema_name: schema_name(namespace, kind)?,
        schema_digest: actual,
    })
}

fn invalid_identity(rule: &'static str, path: &str, detail: &str) -> Error {
    Error::new(
        code::INVALID_SCHEMA_IDENTITY,
        rule,
        Stage::Validate,
        path,
        detail,
    )
}

fn binding(rule: &'static str, detail: &str) -> Error {
    Error::new(
        code::SCHEMA_SOURCE_BINDING,
        rule,
        Stage::Validate,
        "$.schema_digest",
        detail,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_over_the_raw_bytes() {
        // the SHA-256 of the empty input and of "abc" (FIPS 180-4 examples)
        assert_eq!(
            schema_digest(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            schema_digest(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(schema_digest(b"a: 1\n"), schema_digest(b"a: 1"));
        assert_ne!(schema_digest(b"a: 1\n"), schema_digest(b"a: 1\r\n"));
    }

    #[test]
    fn name_is_composed_exactly() {
        assert_eq!(
            schema_name(Some("cic:storage"), Some("StorageResource")).unwrap(),
            "cic:storage:StorageResource"
        );
        assert_eq!(
            schema_name(Some(" Cic:Storage"), Some("kind ")).unwrap(),
            " Cic:Storage:kind "
        );
    }

    #[test]
    fn name_refusals_name_the_component() {
        for (ns, kind, rule) in [
            (None, Some("K"), "R-SCHEMA-NAME-NAMESPACE"),
            (Some(""), Some("K"), "R-SCHEMA-NAME-NAMESPACE"),
            (Some(":a"), Some("K"), "R-SCHEMA-NAME-NAMESPACE"),
            (Some("a:"), Some("K"), "R-SCHEMA-NAME-NAMESPACE"),
            (Some("a::b"), Some("K"), "R-SCHEMA-NAME-NAMESPACE"),
            (Some("a"), None, "R-SCHEMA-NAME-KIND"),
            (Some("a"), Some(""), "R-SCHEMA-NAME-KIND"),
            (Some("a"), Some("K:x"), "R-SCHEMA-NAME-KIND"),
        ] {
            let e = schema_name(ns, kind).unwrap_err();
            assert_eq!(
                (e.code, e.rule),
                (code::INVALID_SCHEMA_IDENTITY, rule),
                "{ns:?} {kind:?}"
            );
        }
    }

    #[test]
    fn binding_checks_the_digest_first() {
        let src = b"spec: {}\n";
        let good = schema_digest(src);
        let e = bind_schema_identity(src, None, None, None).unwrap_err();
        assert_eq!(e.rule, "R-SCHEMA-DIGEST-MISSING");
        let e = bind_schema_identity(src, Some("sha256:00"), None, None).unwrap_err();
        assert_eq!(
            (e.code, e.rule),
            (code::SCHEMA_SOURCE_BINDING, "R-SCHEMA-DIGEST-MISMATCH")
        );
        let e = bind_schema_identity(src, Some(&good), None, Some("K")).unwrap_err();
        assert_eq!(e.rule, "R-SCHEMA-NAME-NAMESPACE");
        let id = bind_schema_identity(src, Some(&good), Some("a:b"), Some("K")).unwrap();
        assert_eq!(id.schema_name, "a:b:K");
        assert_eq!(id.schema_digest, good);
    }
}
