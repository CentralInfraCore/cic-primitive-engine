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
// # Two entry points, and what each one guarantees
//
// EvaluateAndBuild is the primary entry point. It runs plan.Evaluate and
// builds the artifact in one operation, so the plan whose digest is written
// into the artifact is, by construction, the plan that produced the verdicts,
// and the result cannot be edited in between. Use it unless you only hold a
// finished result.
//
// Build is the low-level assembler for a caller that already holds a plan and
// a plan.ObjectVerdict. It checks structure only, and it is exactly as
// trustworthy as the caller who pairs the two:
//
//   - it rejects a result that is not shaped like an Evaluate result for this
//     plan: Fields and Consumed must name the same paths, every Fields path
//     must be one the plan can produce (a planned scalar, or a planned
//     collection element path), every planned scalar must be present, and
//     Object must be what plan.Aggregate gives for Fields;
//   - it does NOT know which plan produced the result. A result evaluated
//     under one plan, paired with another plan that has the same paths and a
//     different comparator, is accepted, and the second plan's digest is
//     written next to the first plan's verdicts;
//   - it does NOT re-evaluate, so a self-consistent but altered result (a
//     changed field verdict with a matching Object, a changed consumed value)
//     is accepted;
//   - it cannot tell a collection element path that Evaluate produced from one
//     that only has the right shape.
//
// So Build must never be described as rejecting every tampered result. Where
// the binding matters, use EvaluateAndBuild.
//
// A Consumed entry that is not a valid ConsumedField (Go's zero value, which
// no constructor produced) is refused as well; Rust cannot construct that
// state at all, so this check has no Rust counterpart.
//
// The checks run in the same order as the Rust peer and walk paths sorted
// byte-wise, so both languages name the same offending path.
package verdict

import (
	"fmt"
	"sort"
	"strings"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/digestprojection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
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

// planCanProduce reports whether Evaluate could have classified path under cp:
// a planned scalar path, or a collection element path, which Evaluate builds
// as "{collection}/{identity}/{element}" with a non-empty identity.
func planCanProduce(cp plan.ConformancePlan, path string) bool {
	for _, fp := range cp.Scalars {
		if fp.Path == path {
			return true
		}
	}
	for _, coll := range cp.Collections {
		head := coll.Path + "/{"
		for _, ef := range coll.Elements {
			tail := "}/" + ef.Path
			if len(path) > len(head)+len(tail) && strings.HasPrefix(path, head) && strings.HasSuffix(path, tail) {
				return true
			}
		}
	}
	return false
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
	for _, path := range sortedPaths(result.Fields) {
		if !planCanProduce(cp, path) {
			return inconsistent(path, "path is not one the plan can produce (no planned scalar or collection element matches)")
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

// EvaluateAndBuild runs plan.Evaluate and builds the verdict artifact in one
// operation: the primary entry point. The plan digest in the artifact is the
// digest of the plan that was actually executed, and the result cannot be
// swapped or edited between evaluation and assembly.
//
// Like plan.Evaluate it panics if cp is malformed. The error is as Build's; a
// result produced here is always consistent, so in practice only a
// canonicalization error from the digest functions.
func EvaluateAndBuild(
	intent, observed materialized.MaterializedObject,
	obs *conformance.Observation,
	cp plan.ConformancePlan,
) (Verdict, error) {
	return Build(cp, plan.Evaluate(intent, observed, obs, cp))
}

// Build assembles the verdict artifact for result, claimed to be the output
// of plan.Evaluate run against cp. Low-level: it checks structure only and
// cannot verify that cp is the plan that produced result -- see the package
// doc for exactly what it does and does not reject, and prefer
// EvaluateAndBuild.
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
