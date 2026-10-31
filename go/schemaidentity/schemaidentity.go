// Package schemaidentity defines the domain schema identity of a receipt (D1.3, docs/RECEIPT-SCHEMA.md "Schema identity contract") -- the Go peer
// of this repo's engine/src/schema_identity.rs.
//
// It defines what two receipt fields mean; it does not read a schema, authenticate where one came from, or verify a receipt.
//
//   - SchemaDigest is "sha256:" and the lowercase hex of the SHA-256 of the raw bytes of the pinned schema source file. Nothing is parsed,
//     normalized or re-serialized first, so comments, key order, line endings and a missing final newline are all part of the identity. It is
//     spelled like the A7 digest of a section A value but is not one.
//   - SchemaName is identity.namespace + ":" + identity.kind, both read from the pinned schema's spec.identity and used as authored.
//
// The caller passes the identity components (this module has no schema reader). A missing component is nil, an empty one is a pointer to "",
// and both are refused. The order of the checks is part of the contract: the digest binding, then the namespace, then the kind.
package schemaidentity

import (
	"fmt"
	"strings"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
)

// Rejection codes and rules, spelled as in the Rust peer.
const (
	CodeInvalidSchemaIdentity = "E_INVALID_SCHEMA_IDENTITY"
	CodeSchemaSourceBinding   = "E_SCHEMA_SOURCE_BINDING"
)

// Error is a rejection with the code/rule/path fields the Rust peer's Error carries.
type Error struct {
	Code   string
	Rule   string
	Path   string
	Detail string
}

func (e *Error) Error() string {
	return fmt.Sprintf("%s (%s) at %s: %s", e.Code, e.Rule, e.Path, e.Detail)
}

// Identity is the pair of receipt fields this contract defines.
type Identity struct {
	SchemaName   string
	SchemaDigest string
}

// SchemaDigest is "sha256:" and the lowercase hex SHA-256 of source, taken over the bytes as they are.
func SchemaDigest(source []byte) string { return canonical.Digest(source) }

// SchemaName is namespace + ":" + kind, exactly as given, if both are well formed. The namespace is non-empty and made of non-empty
// components separated by ":"; the kind is non-empty and holds no ":". Nothing is trimmed, case-folded or escaped.
func SchemaName(namespace, kind *string) (string, error) {
	if namespace == nil {
		return "", invalidIdentity("R-SCHEMA-NAME-NAMESPACE", "$.spec.identity.namespace", "the namespace is absent")
	}
	if *namespace == "" || hasEmptyComponent(*namespace) {
		return "", invalidIdentity("R-SCHEMA-NAME-NAMESPACE", "$.spec.identity.namespace", "the namespace is empty or has an empty component")
	}
	if kind == nil {
		return "", invalidIdentity("R-SCHEMA-NAME-KIND", "$.spec.identity.kind", "the kind is absent")
	}
	if *kind == "" || strings.Contains(*kind, ":") {
		return "", invalidIdentity("R-SCHEMA-NAME-KIND", "$.spec.identity.kind", "the kind is empty or contains a colon")
	}
	return *namespace + ":" + *kind, nil
}

func hasEmptyComponent(s string) bool {
	for _, part := range strings.Split(s, ":") {
		if part == "" {
			return true
		}
	}
	return false
}

// BindSchemaIdentity binds claimedDigest to the source bytes it must describe and returns the identity of the schema. A claim that is absent
// (nil), or that is not exactly the digest of source, is refused: the digest of other bytes never stands in for the source. Consistency only:
// where source came from is not established here.
func BindSchemaIdentity(source []byte, claimedDigest, namespace, kind *string) (Identity, error) {
	if claimedDigest == nil {
		return Identity{}, binding("R-SCHEMA-DIGEST-MISSING", "no digest is claimed for the schema source")
	}
	actual := SchemaDigest(source)
	if *claimedDigest != actual {
		return Identity{}, binding("R-SCHEMA-DIGEST-MISMATCH", "the claimed digest is not the digest of the source bytes")
	}
	name, err := SchemaName(namespace, kind)
	if err != nil {
		return Identity{}, err
	}
	return Identity{SchemaName: name, SchemaDigest: actual}, nil
}

func invalidIdentity(rule, path, detail string) *Error {
	return &Error{Code: CodeInvalidSchemaIdentity, Rule: rule, Path: path, Detail: detail}
}

func binding(rule, detail string) *Error {
	return &Error{Code: CodeSchemaSourceBinding, Rule: rule, Path: "$.schema_digest", Detail: detail}
}
