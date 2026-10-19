// Package receipt verifies a materialization receipt (docs/RECEIPT-SCHEMA.md)
// -- the Go peer of this repo's own engine/src/receipt.rs.
//
// This package checks a receipt that somebody else produced. It never
// produces one: materialization, and the evidence a default or a derivation
// leaves while it runs, belong to the environment (docs/BOUNDARY.md).
//
// # What a successful result establishes
//
// Structural and content consistency, and nothing more. A receipt that passes
// Verify:
//
//   - has all 15 required top-level fields, with the right types, a supported
//     receipt_schema_version (1) and every digest spelled "sha256:" + 64
//     lowercase hex characters;
//   - lists applied_defaults and derived_values in strictly ascending .path
//     order, and each inputs list as a strictly ascending (deduplicated) path
//     set;
//   - carries an input_digest equal to the A7 digest of the canonical form of
//     the authored input it was handed, and an output_digest equal to
//     digestprojection.MaterializedObjectDigest of the object it was handed;
//   - lists exactly the defaulted and the derived fields of that object, each
//     with the digest of the field's value, through the explicit PathBinding
//     the caller supplied.
//
// It does NOT establish that a default was applied or a derivation ran, that
// rule or inputs describe what actually happened, who produced the receipt,
// or that any identifier is genuine:
//
//   - grammar_*, primitive_*, schema_* and engine_* are claims. Their form is
//     checked; the release signature and the origin of the pinned schema are
//     not (external authentication is not this package's job).
//   - engine_identity names the implementation that materialized the data,
//     not this verifier.
//   - No signature is checked. The receipt schema has no signature field.
//
// # input_digest
//
// The caller passes the processed authored input as a canonical value
// (canonical.IsValue). The digest is taken over that value's canonical JSON
// (section A), not over raw source bytes, and no YAML reader is assumed.
//
// # Evidence needs an explicit path binding
//
// A MaterializedObject is keyed by flat field names. A receipt names fields by
// path ("$.config.replicas"). This package does not guess a mapping from one
// to the other. When the receipt lists any evidence entry, the caller must
// pass a PathBinding from receipt path to object key; an entry whose path is
// not in the binding is refused with CodeReceiptUnbound (fail-closed), never
// accepted partially. With both evidence lists empty and no defaulted or
// derived field in the object, no binding is needed.
//
// derived_values[].inputs is checked for form and order only. The finished
// object cannot show which paths a derivation actually read, so no input path
// is required to exist in the object.
//
// # Parsing a document
//
// Parse takes a decoded document. Integers must arrive as json.Number (decode
// with Decoder.UseNumber), int or int64; a float64 -- even 1.0 -- is not an
// integer, matching the Rust reader, which reads 1.0 as a float.
//
// # Check order
//
// Shape and structure first (declared field order), then input_digest, then
// output_digest, then evidence entries in list order, then the object's own
// defaulted/derived fields in key order. The Rust peer runs the same order, so
// both name the same offending location.
package receipt

import (
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/digestprojection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
)

// SchemaVersion is the only receipt_schema_version this verifier supports.
const SchemaVersion int64 = 1

// DefaultRule is DefaultEvidence.rule: a literal v1 constant (C5).
const DefaultRule = "schema-default"

// Rejection codes -- engine/src/error.rs calls these INVALID_RECEIPT,
// RECEIPT_MISMATCH and RECEIPT_UNBOUND.
const (
	CodeInvalidReceipt  = "E_INVALID_RECEIPT"
	CodeReceiptMismatch = "E_RECEIPT_MISMATCH"
	CodeReceiptUnbound  = "E_RECEIPT_UNBOUND"
)

const wrapper = "materialization"

// fields are the 15 required top-level fields, in declared order.
var fields = []string{
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
}

// PathBinding maps a receipt path to a MaterializedObject key, supplied by
// the environment.
type PathBinding map[string]string

// DefaultEvidence is an applied_defaults[] entry.
type DefaultEvidence struct {
	Path        string
	Rule        string
	ValueDigest string
}

// DerivedEvidence is a derived_values[] entry.
type DerivedEvidence struct {
	Path        string
	Rule        string
	Inputs      []string
	ValueDigest string
}

// Receipt is the materialization receipt: 15 required fields, none optional.
type Receipt struct {
	ReceiptSchemaVersion  int64
	GrammarSHA256         string
	GrammarSchemaSHA256   string
	PrimitiveRelease      string
	PrimitiveSourceCommit string
	PrimitiveBuildHash    string
	SchemaName            string
	SchemaVersion         string
	SchemaDigest          string
	EngineIdentity        string
	EngineVersion         string
	InputDigest           string
	OutputDigest          string
	AppliedDefaults       []DefaultEvidence
	DerivedValues         []DerivedEvidence
}

// Error is a rejection with the same code/rule/path fields the Rust peer's
// Error carries.
type Error struct {
	Code   string
	Rule   string
	Path   string
	Detail string
}

func (e *Error) Error() string {
	return fmt.Sprintf("%s (%s) at %s: %s", e.Code, e.Rule, e.Path, e.Detail)
}

func invalid(rule, path, detail string) *Error {
	return &Error{Code: CodeInvalidReceipt, Rule: rule, Path: path, Detail: detail}
}

func mismatch(rule, path, detail string) *Error {
	return &Error{Code: CodeReceiptMismatch, Rule: rule, Path: path, Detail: detail}
}

func fieldPath(name string) string { return "$." + wrapper + "." + name }

func shape(path, want string) *Error {
	return invalid("R-RECEIPT-SHAPE", path, "expected "+want)
}

const absent = "this required field (it is absent)"

const emptyNotNull = "a sequence, `[]` when empty, never null"

func takeStr(m map[string]interface{}, key, path string) (string, *Error) {
	v, ok := m[key]
	if !ok {
		return "", shape(path, absent)
	}
	s, ok := v.(string)
	if !ok {
		return "", shape(path, "a string")
	}
	return s, nil
}

func takeMap(v interface{}, path string) (map[string]interface{}, *Error) {
	m, ok := v.(map[string]interface{})
	if !ok {
		return nil, shape(path, "a mapping")
	}
	return m, nil
}

func checkKeys(m map[string]interface{}, allowed []string, path string) *Error {
	var extra []string
	for k := range m {
		known := false
		for _, a := range allowed {
			if a == k {
				known = true
				break
			}
		}
		if !known {
			extra = append(extra, k)
		}
	}
	if len(extra) == 0 {
		return nil
	}
	sort.Strings(extra)
	return shape(path+"."+extra[0], "no member outside the receipt layout")
}

// integer reads an integer from a decoded value; a float64 is not one.
func integer(v interface{}) (int64, bool) {
	switch n := v.(type) {
	case json.Number:
		i, err := strconv.ParseInt(n.String(), 10, 64)
		return i, err == nil
	case int:
		return int64(n), true
	case int64:
		return n, true
	default:
		return 0, false
	}
}

// Parse reads a receipt document: a mapping whose only key is
// "materialization", holding the 15 fields. Shape errors (missing or unknown
// member, wrong type) are raised here, in declared field order; form and
// order errors are raised by CheckStructure, which Parse calls last.
func Parse(doc interface{}) (Receipt, error) {
	var r Receipt
	top, err := takeMap(doc, "$")
	if err != nil {
		return r, err
	}
	bodyRaw, ok := top[wrapper]
	if !ok {
		return r, shape("$", "the sole wrapper key `materialization`")
	}
	if err := checkKeys(top, []string{wrapper}, "$"); err != nil {
		return r, err
	}
	base := "$." + wrapper
	body, err := takeMap(bodyRaw, base)
	if err != nil {
		return r, err
	}

	// Required arrays are never null: [] must come out as a non-nil empty slice.
	r.AppliedDefaults = []DefaultEvidence{}
	r.DerivedValues = []DerivedEvidence{}
	strs := map[string]string{}
	for _, name := range fields {
		path := fieldPath(name)
		switch name {
		case "receipt_schema_version":
			raw, ok := body[name]
			if !ok {
				return r, shape(path, absent)
			}
			n, ok := integer(raw)
			if !ok {
				return r, shape(path, "an integer")
			}
			r.ReceiptSchemaVersion = n
		case "applied_defaults":
			raw, ok := body[name]
			if !ok {
				return r, shape(path, absent)
			}
			items, ok := raw.([]interface{})
			if !ok {
				return r, shape(path, "a sequence")
			}
			for i, item := range items {
				e, err := parseDefault(item, fmt.Sprintf("%s[%d]", path, i))
				if err != nil {
					return r, err
				}
				r.AppliedDefaults = append(r.AppliedDefaults, e)
			}
		case "derived_values":
			raw, ok := body[name]
			if !ok {
				return r, shape(path, absent)
			}
			items, ok := raw.([]interface{})
			if !ok {
				return r, shape(path, "a sequence")
			}
			for i, item := range items {
				e, err := parseDerived(item, fmt.Sprintf("%s[%d]", path, i))
				if err != nil {
					return r, err
				}
				r.DerivedValues = append(r.DerivedValues, e)
			}
		default:
			s, err := takeStr(body, name, path)
			if err != nil {
				return r, err
			}
			strs[name] = s
		}
	}
	if err := checkKeys(body, fields, base); err != nil {
		return r, err
	}

	r.GrammarSHA256 = strs["grammar_sha256"]
	r.GrammarSchemaSHA256 = strs["grammar_schema_sha256"]
	r.PrimitiveRelease = strs["primitive_release"]
	r.PrimitiveSourceCommit = strs["primitive_source_commit"]
	r.PrimitiveBuildHash = strs["primitive_build_hash"]
	r.SchemaName = strs["schema_name"]
	r.SchemaVersion = strs["schema_version"]
	r.SchemaDigest = strs["schema_digest"]
	r.EngineIdentity = strs["engine_identity"]
	r.EngineVersion = strs["engine_version"]
	r.InputDigest = strs["input_digest"]
	r.OutputDigest = strs["output_digest"]
	if err := r.CheckStructure(); err != nil {
		return r, err
	}
	return r, nil
}

func parseDefault(v interface{}, at string) (DefaultEvidence, *Error) {
	var e DefaultEvidence
	m, err := takeMap(v, at)
	if err != nil {
		return e, err
	}
	if e.Path, err = takeStr(m, "path", at+".path"); err != nil {
		return e, err
	}
	if e.Rule, err = takeStr(m, "rule", at+".rule"); err != nil {
		return e, err
	}
	if e.ValueDigest, err = takeStr(m, "value_digest", at+".value_digest"); err != nil {
		return e, err
	}
	if err := checkKeys(m, []string{"path", "rule", "value_digest"}, at); err != nil {
		return e, err
	}
	return e, nil
}

func parseDerived(v interface{}, at string) (DerivedEvidence, *Error) {
	var e DerivedEvidence
	m, err := takeMap(v, at)
	if err != nil {
		return e, err
	}
	if e.Path, err = takeStr(m, "path", at+".path"); err != nil {
		return e, err
	}
	if e.Rule, err = takeStr(m, "rule", at+".rule"); err != nil {
		return e, err
	}
	inputsAt := at + ".inputs"
	raw, ok := m["inputs"]
	if !ok {
		return e, shape(inputsAt, absent)
	}
	items, ok := raw.([]interface{})
	if !ok {
		return e, shape(inputsAt, "a sequence")
	}
	e.Inputs = make([]string, 0, len(items))
	for j, item := range items {
		s, ok := item.(string)
		if !ok {
			return e, shape(fmt.Sprintf("%s[%d]", inputsAt, j), "a string")
		}
		e.Inputs = append(e.Inputs, s)
	}
	if e.ValueDigest, err = takeStr(m, "value_digest", at+".value_digest"); err != nil {
		return e, err
	}
	if err := checkKeys(m, []string{"path", "rule", "inputs", "value_digest"}, at); err != nil {
		return e, err
	}
	return e, nil
}

// CheckStructure runs the form, version and ordering checks that need no
// other artifact.
func (r Receipt) CheckStructure() error {
	if r.ReceiptSchemaVersion != SchemaVersion {
		return invalid("R-RECEIPT-VERSION", fieldPath("receipt_schema_version"),
			fmt.Sprintf("unsupported receipt_schema_version %d (supported: %d)", r.ReceiptSchemaVersion, SchemaVersion))
	}
	steps := []func() *Error{
		func() *Error { return checkDigest(r.GrammarSHA256, "grammar_sha256") },
		func() *Error { return checkDigest(r.GrammarSchemaSHA256, "grammar_schema_sha256") },
		func() *Error { return checkNonEmpty(r.PrimitiveRelease, "primitive_release") },
		func() *Error { return checkNonEmpty(r.PrimitiveSourceCommit, "primitive_source_commit") },
		func() *Error { return checkDigest(r.PrimitiveBuildHash, "primitive_build_hash") },
		func() *Error { return checkNonEmpty(r.SchemaName, "schema_name") },
		func() *Error { return checkNonEmpty(r.SchemaVersion, "schema_version") },
		func() *Error { return checkDigest(r.SchemaDigest, "schema_digest") },
		func() *Error { return checkNonEmpty(r.EngineIdentity, "engine_identity") },
		func() *Error { return checkNonEmpty(r.EngineVersion, "engine_version") },
		func() *Error { return checkDigest(r.InputDigest, "input_digest") },
		func() *Error { return checkDigest(r.OutputDigest, "output_digest") },
	}
	for _, step := range steps {
		if err := step(); err != nil {
			return err
		}
	}

	// The evidence lists are required arrays: `[]` when empty, never null. A
	// nil slice is Go's spelling of null/absent, so the typed API refuses it.
	if r.AppliedDefaults == nil {
		return shape(fieldPath("applied_defaults"), emptyNotNull)
	}
	for i, e := range r.AppliedDefaults {
		at := fmt.Sprintf("%s[%d]", fieldPath("applied_defaults"), i)
		if err := checkPathForm(e.Path, at+".path"); err != nil {
			return err
		}
		if e.Rule != DefaultRule {
			return invalid("R-RECEIPT-EVIDENCE-FORM", at+".rule",
				fmt.Sprintf("rule must be the literal %q, got %q", DefaultRule, e.Rule))
		}
		if err := checkDigestAt(e.ValueDigest, at+".value_digest"); err != nil {
			return err
		}
		if i > 0 {
			if err := checkAscending(r.AppliedDefaults[i-1].Path, e.Path, at+".path"); err != nil {
				return err
			}
		}
	}
	if r.DerivedValues == nil {
		return shape(fieldPath("derived_values"), emptyNotNull)
	}
	for i, e := range r.DerivedValues {
		at := fmt.Sprintf("%s[%d]", fieldPath("derived_values"), i)
		if err := checkPathForm(e.Path, at+".path"); err != nil {
			return err
		}
		if err := checkDerivedRule(e.Rule, at+".rule"); err != nil {
			return err
		}
		if e.Inputs == nil {
			return shape(at+".inputs", emptyNotNull)
		}
		for j, input := range e.Inputs {
			iat := fmt.Sprintf("%s.inputs[%d]", at, j)
			if err := checkPathForm(input, iat); err != nil {
				return err
			}
			if j > 0 {
				if err := checkAscending(e.Inputs[j-1], input, iat); err != nil {
					return err
				}
			}
		}
		if err := checkDigestAt(e.ValueDigest, at+".value_digest"); err != nil {
			return err
		}
		if i > 0 {
			if err := checkAscending(r.DerivedValues[i-1].Path, e.Path, at+".path"); err != nil {
				return err
			}
		}
	}
	return nil
}

func isDigest(s string) bool {
	hex, ok := strings.CutPrefix(s, "sha256:")
	if !ok || len(hex) != 64 {
		return false
	}
	for i := 0; i < len(hex); i++ {
		c := hex[i]
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}

func checkDigestAt(s, path string) *Error {
	if isDigest(s) {
		return nil
	}
	return invalid("R-RECEIPT-DIGEST-FORMAT", path,
		"a digest is `sha256:` followed by 64 lowercase hex characters")
}

func checkDigest(s, name string) *Error { return checkDigestAt(s, fieldPath(name)) }

func checkNonEmpty(s, name string) *Error {
	if s == "" {
		return shape(fieldPath(name), "a non-empty string")
	}
	return nil
}

// checkPathForm accepts "$." followed by one or more non-empty dot-separated
// segments.
func checkPathForm(p, at string) *Error {
	rest, ok := strings.CutPrefix(p, "$.")
	if ok {
		for _, seg := range strings.Split(rest, ".") {
			if seg == "" {
				ok = false
				break
			}
		}
	}
	if ok {
		return nil
	}
	return invalid("R-RECEIPT-EVIDENCE-FORM", at,
		fmt.Sprintf("%q is not a `$.`-rooted path with non-empty segments", p))
}

// checkDerivedRule accepts name + "-v" + one or more ASCII digits, with a
// non-empty name.
func checkDerivedRule(rule, at string) *Error {
	idx := strings.LastIndex(rule, "-v")
	ok := idx > 0
	if ok {
		n := rule[idx+2:]
		ok = n != ""
		for i := 0; ok && i < len(n); i++ {
			ok = n[i] >= '0' && n[i] <= '9'
		}
	}
	if ok {
		return nil
	}
	return invalid("R-RECEIPT-EVIDENCE-FORM", at,
		fmt.Sprintf("%q is not a versioned rule identifier (`name-vN`)", rule))
}

// checkAscending requires strictly ascending raw UTF-8 byte order: equal
// means a duplicate, smaller means out of order.
func checkAscending(prev, cur, at string) *Error {
	if prev < cur {
		return nil
	}
	what := "out of order"
	if prev == cur {
		what = "duplicate"
	}
	return invalid("R-RECEIPT-ORDER", at,
		fmt.Sprintf("%q is %s after %q (strictly ascending byte order required)", cur, what, prev))
}

type kind int

const (
	kindOther kind = iota
	kindDefault
	kindDerived
)

func fieldKind(f materialized.MaterializedField) kind {
	if f.Evidence.IsDerived() {
		return kindDerived
	}
	if p, ok := f.Evidence.Provenance(); ok {
		switch p {
		case materialized.ProvenanceSchemaDefault:
			return kindDefault
		case materialized.ProvenanceDerived:
			return kindDerived
		}
	}
	return kindOther
}

func unbound(at, path string) *Error {
	return &Error{
		Code:   CodeReceiptUnbound,
		Rule:   "R-RECEIPT-PATH-BINDING",
		Path:   at,
		Detail: fmt.Sprintf("receipt path %q has no explicit binding to a materialized object key", path),
	}
}

// checkEntry resolves one evidence entry to its bound field and compares the
// value digest. want is the kind the entry's list requires.
func checkEntry(obj materialized.MaterializedObject, binding PathBinding, path, valueDigest string, want kind, at string) (string, error) {
	key, ok := binding[path]
	if !ok {
		return "", unbound(at+".path", path)
	}
	field, ok := obj.Get(key)
	if !ok {
		return "", mismatch("R-RECEIPT-EVIDENCE-MEMBERSHIP", at+".path",
			fmt.Sprintf("%q is bound to %q, which is not a key of the object", path, key))
	}
	if fieldKind(field) != want {
		return "", mismatch("R-RECEIPT-EVIDENCE-MEMBERSHIP", at+".path",
			fmt.Sprintf("object field %q does not have the provenance this list requires", key))
	}
	value, has := field.Value()
	if !has {
		return "", mismatch("R-RECEIPT-EVIDENCE-VALUE", at+".value_digest",
			fmt.Sprintf("object field %q carries no value to digest", key))
	}
	b, err := canonical.ToCanonicalJSON(value)
	if err != nil {
		return "", err
	}
	if got := canonical.Digest(b); got != valueDigest {
		return "", mismatch("R-RECEIPT-EVIDENCE-VALUE", at+".value_digest",
			fmt.Sprintf("value digest of %q is %s, the receipt says %s", key, got, valueDigest))
	}
	return key, nil
}

// Verify checks r against the object it describes and the authored input it
// was made from. See the package documentation for exactly what a success
// does and does not establish.
//
// binding maps receipt paths to object keys; it may be empty only when the
// receipt lists no evidence. The returned error is an *Error, except for a
// canonicalization failure of authoredInput or of the object, which is
// returned as that error.
func Verify(r Receipt, obj materialized.MaterializedObject, authoredInput interface{}, binding PathBinding) error {
	if err := r.CheckStructure(); err != nil {
		return err
	}

	inputBytes, err := canonical.ToCanonicalJSON(authoredInput)
	if err != nil {
		return err
	}
	if got := canonical.Digest(inputBytes); got != r.InputDigest {
		return mismatch("R-RECEIPT-BINDS-INPUT", fieldPath("input_digest"),
			"canonical digest of the authored input is "+got)
	}
	got, err := digestprojection.MaterializedObjectDigest(obj)
	if err != nil {
		return err
	}
	if got != r.OutputDigest {
		return mismatch("R-RECEIPT-BINDS-OUTPUT", fieldPath("output_digest"),
			"digest of the materialized object is "+got)
	}

	defaulted := map[string]struct{}{}
	for i, e := range r.AppliedDefaults {
		at := fmt.Sprintf("%s[%d]", fieldPath("applied_defaults"), i)
		key, cerr := checkEntry(obj, binding, e.Path, e.ValueDigest, kindDefault, at)
		if cerr != nil {
			return cerr
		}
		if _, dup := defaulted[key]; dup {
			return mismatch("R-RECEIPT-EVIDENCE-MEMBERSHIP", at+".path",
				fmt.Sprintf("object field %q is listed twice", key))
		}
		defaulted[key] = struct{}{}
	}
	derived := map[string]struct{}{}
	for i, e := range r.DerivedValues {
		at := fmt.Sprintf("%s[%d]", fieldPath("derived_values"), i)
		key, cerr := checkEntry(obj, binding, e.Path, e.ValueDigest, kindDerived, at)
		if cerr != nil {
			return cerr
		}
		if _, dup := derived[key]; dup {
			return mismatch("R-RECEIPT-EVIDENCE-MEMBERSHIP", at+".path",
				fmt.Sprintf("object field %q is listed twice", key))
		}
		derived[key] = struct{}{}
	}

	for _, key := range obj.Keys() {
		field, _ := obj.Get(key)
		var listed bool
		var list string
		switch fieldKind(field) {
		case kindDefault:
			_, listed = defaulted[key]
			list = "applied_defaults"
		case kindDerived:
			_, listed = derived[key]
			list = "derived_values"
		default:
			continue
		}
		if !listed {
			return mismatch("R-RECEIPT-EVIDENCE-MEMBERSHIP", "object/"+key,
				fmt.Sprintf("the object's field %q has no entry in %s", key, list))
		}
	}
	return nil
}
