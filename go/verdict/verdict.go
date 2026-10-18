// Package verdict assembles the conformance/drift verdict artifact into the
// single five-field shape docs/VERDICT-SCHEMA.md commits to -- the Go peer of
// this repo's own engine/src/verdict.rs.
//
// plan.Evaluate produces the semantic core (Object, Fields, and the Consumed
// map it recorded while classifying) and go/digestprojection produces the two
// digests from a plan and from that Consumed map. Nothing bundled those into
// the one required artifact until this package.
//
// This package decides nothing new:
//
//   - Object and Fields are Evaluate's own result, carried over verbatim;
//   - ConformancePlanDigest is digestprojection.ConformancePlanDigest of the
//     plan that was executed and ObservationDigest is
//     digestprojection.ObservationDigest of the Consumed map Evaluate
//     recorded -- the existing normative projections, called, never
//     recomputed or re-derived here;
//   - SchemaVersion is the bare integer 1 (VERDICT-SCHEMA.md).
//
// # What is checked before an artifact exists
//
// The schema says Fields holds exactly one entry per path Evaluate
// classified, and that every field is required. Build therefore refuses an
// ObjectVerdict that cannot be such a result, rather than digesting it into
// an artifact that would look authoritative:
//
//   - the Fields and Consumed path sets must be identical;
//   - every plan scalar path must be in Fields;
//   - Object must be what plan.Aggregate gives for Fields.
//
// A Consumed entry that is not a valid ConsumedField (Go's zero value, which
// no constructor produced) is refused the same way; Rust cannot construct
// that state at all, so this check has no Rust counterpart.
//
// The checks run in the same order as the Rust peer and walk paths sorted
// byte-wise, so both languages name the same offending path.
package verdict

import (
	"fmt"
	"sort"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/digestprojection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

// SchemaVersion is the only verdict_schema_version this package produces.
// Incremented on any backward-incompatible change to the verdict contract.
const SchemaVersion int64 = 1

// CodeInconsistentVerdict is the rejection code engine/src/error.rs calls
// INCONSISTENT_VERDICT ("E_INCONSISTENT_VERDICT").
const CodeInconsistentVerdict = "E_INCONSISTENT_VERDICT"

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

func inconsistent(path, detail string) *Error {
	return &Error{Code: CodeInconsistentVerdict, Rule: "R-VERDICT-CONSISTENT", Path: path, Detail: detail}
}

// Verdict holds the five required fields. There is no optional member.
type Verdict struct {
	SchemaVersion         int64
	Object                plan.ObjectConformance
	Fields                map[string]conformance.FieldVerdict
	ConformancePlanDigest string
	ObservationDigest     string
}

func sortedPaths[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func checkConsistent(cp plan.ConformancePlan, result plan.ObjectVerdict) error {
	for _, path := range sortedPaths(result.Fields) {
		if _, ok := result.Consumed[path]; !ok {
			return inconsistent(path, "classified path has no consumed observation evidence")
		}
	}
	for _, path := range sortedPaths(result.Consumed) {
		if _, ok := result.Fields[path]; !ok {
			return inconsistent(path, "consumed observation evidence for a path that was not classified")
		}
		if !result.Consumed[path].Valid() {
			return inconsistent(path, "consumed observation evidence is not a valid ConsumedField")
		}
	}
	for _, fp := range cp.Scalars {
		if _, ok := result.Fields[fp.Path]; !ok {
			return inconsistent(fp.Path, "planned scalar path is missing from fields")
		}
	}
	if want := plan.Aggregate(result.Fields); want != result.Object {
		return inconsistent("$", fmt.Sprintf(
			"object %s does not follow from fields (expected %s)", result.Object, want))
	}
	return nil
}

// Build assembles the verdict artifact for result, the output of
// plan.Evaluate run against cp.
//
// Returns an *Error with code E_INCONSISTENT_VERDICT if result cannot be an
// Evaluate result for cp (see the package doc), or the canonicalization error
// of the digest functions.
func Build(cp plan.ConformancePlan, result plan.ObjectVerdict) (Verdict, error) {
	if err := checkConsistent(cp, result); err != nil {
		return Verdict{}, err
	}
	planDigest, err := digestprojection.ConformancePlanDigest(cp)
	if err != nil {
		return Verdict{}, err
	}
	obsDigest, err := digestprojection.ObservationDigest(result.Consumed)
	if err != nil {
		return Verdict{}, err
	}
	fields := make(map[string]conformance.FieldVerdict, len(result.Fields))
	for k, v := range result.Fields {
		fields[k] = v
	}
	return Verdict{
		SchemaVersion:         SchemaVersion,
		Object:                result.Object,
		Fields:                fields,
		ConformancePlanDigest: planDigest,
		ObservationDigest:     obsDigest,
	}, nil
}

// Value is the artifact as a value tree: exactly the five required keys.
func (v Verdict) Value() map[string]interface{} {
	fields := make(map[string]interface{}, len(v.Fields))
	for path, fv := range v.Fields {
		fields[path] = string(fv)
	}
	return map[string]interface{}{
		"verdict_schema_version":  v.SchemaVersion,
		"object":                  string(v.Object),
		"fields":                  fields,
		"conformance_plan_digest": v.ConformancePlanDigest,
		"observation_digest":      v.ObservationDigest,
	}
}

// CanonicalBytes is the artifact's canonical bytes (section A): the value
// tree of Value through canonical.ToCanonicalJSON.
func (v Verdict) CanonicalBytes() ([]byte, error) {
	return canonical.ToCanonicalJSON(v.Value())
}
