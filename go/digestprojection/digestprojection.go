// Package digestprojection wires F5's two digest preimages
// (docs/MATERIALIZATION-SPEC.md) to the real ConformancePlan (go/plan)
// and the real per-path coverage/value data Evaluate records
// (go/plan's ObjectVerdict.Consumed) -- the Go peer of this repo's own
// engine/src/digest_projection.rs (F9).
//
// This package does not decide anything new: F5 already specified
// both projections' exact value-tree shape and ordering rule, and
// digest_projection.rs already wired it to the Rust types. It only
// builds the identical shape from go/plan's own types and digests it
// with go/canonical, rather than inventing a fresh one.
//
// # Sorting is this package's own job here, not free
//
// digest_projection.rs's own doc comment notes that Consumed is
// already a BTreeMap<String, _>, so its iteration order already IS
// the byte-wise-by-path sort F5 requires -- "this function does not
// re-sort what's already sorted." go/plan deliberately chose a plain
// Go map for ObjectVerdict.Consumed instead (see that package's own
// doc comment: "a future caller... sorts keys at the point it
// actually needs them, rather than this type carrying a sorted
// container everywhere just in case"). This package is exactly that
// future caller, and that point: ObservationDigestProjection sorts
// consumed's keys itself, where the Rust side gets the sort for free.
package digestprojection

import (
	"sort"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

// PlanDigestProjection builds F5's PlanDigestProjection shape:
//
//	map[string]interface{}{
//	  "scalars":     []interface{}{ map{"path", "compare"}, ... },
//	  "collections": []interface{}{ map{"path", "topology", "keys", "elements"}, ... },
//	}
//
// scalars/collections/every elements list sorted by path, byte-wise;
// each collection's keys sorted byte-wise too -- F5's own ordering
// rule, reused here rather than re-decided.
//
// Panics if cp is malformed (see plan.ConformancePlan.Validate's own
// doc comment) -- this is a second, independent entry point for a
// ConformancePlan, not only reachable through plan.Evaluate, so it
// validates on its own rather than trusting a caller who may never
// have called Evaluate at all. Without this, an invalid CompareType
// string would flow straight into the "compare" field below: unlike a
// value digested through canonical.ToCanonicalJSON's own exhaustive
// type switch, a bare string(fp.Compare) conversion never rejects
// anything -- "garbage" is just as valid a Go string as "exact" is.
func PlanDigestProjection(cp plan.ConformancePlan) map[string]interface{} {
	cp.Validate()

	scalars := append([]plan.FieldPlan(nil), cp.Scalars...)
	sort.Slice(scalars, func(i, j int) bool { return scalars[i].Path < scalars[j].Path })
	scalarEntries := make([]interface{}, len(scalars))
	for i, fp := range scalars {
		scalarEntries[i] = fieldPlanEntry(fp)
	}

	collections := append([]plan.CollectionPlan(nil), cp.Collections...)
	sort.Slice(collections, func(i, j int) bool { return collections[i].Path < collections[j].Path })
	collectionEntries := make([]interface{}, len(collections))
	for i, coll := range collections {
		collectionEntries[i] = collectionPlanEntry(coll)
	}

	return map[string]interface{}{
		"scalars":     scalarEntries,
		"collections": collectionEntries,
	}
}

func fieldPlanEntry(fp plan.FieldPlan) map[string]interface{} {
	return map[string]interface{}{
		"path":    fp.Path,
		"compare": string(fp.Compare),
	}
}

func collectionPlanEntry(cp plan.CollectionPlan) map[string]interface{} {
	keys := append([]string(nil), cp.Collection.Keys...)
	sort.Strings(keys)
	keyEntries := make([]interface{}, len(keys))
	for i, k := range keys {
		keyEntries[i] = k
	}

	elements := append([]plan.FieldPlan(nil), cp.Elements...)
	sort.Slice(elements, func(i, j int) bool { return elements[i].Path < elements[j].Path })
	elementEntries := make([]interface{}, len(elements))
	for i, fp := range elements {
		elementEntries[i] = fieldPlanEntry(fp)
	}

	return map[string]interface{}{
		"path":     cp.Path,
		"topology": string(cp.Collection.Topology),
		"keys":     keyEntries,
		"elements": elementEntries,
	}
}

// ConformancePlanDigest is
// digest(to_canonical_json(PlanDigestProjection)) (F5, verbatim).
func ConformancePlanDigest(cp plan.ConformancePlan) (string, error) {
	b, err := canonical.ToCanonicalJSON(PlanDigestProjection(cp))
	if err != nil {
		return "", err
	}
	return canonical.Digest(b), nil
}

// ObservationDigestProjection builds F5's ObservationDigestProjection
// shape:
//
//	map[string]interface{}{
//	  "fields": []interface{}{ map{"path", "coverage", "value"?}, ... },
//	}
//
// value is present IFF coverage == "observed" (F5's own rule, grounded
// in ClassifyFieldValue's logic, not re-decided here) -- enforced by
// ConsumedField's own closed construction (go/plan, F13): there is no
// way to obtain a ConsumedField whose Value() returns ok=true for any
// other coverage. fields sorted by path, byte-wise -- see the package
// doc for why this package does the sorting go/plan's own Consumed map
// does not do for it.
func ObservationDigestProjection(consumed map[string]plan.ConsumedField) map[string]interface{} {
	paths := make([]string, 0, len(consumed))
	for p := range consumed {
		paths = append(paths, p)
	}
	sort.Strings(paths)

	fields := make([]interface{}, 0, len(paths))
	for _, p := range paths {
		cf := consumed[p]
		entry := map[string]interface{}{
			"path":     p,
			"coverage": string(cf.Coverage()),
		}
		if v, ok := cf.Value(); ok {
			entry["value"] = v
		}
		fields = append(fields, entry)
	}

	return map[string]interface{}{"fields": fields}
}

// ObservationDigest is
// digest(to_canonical_json(ObservationDigestProjection)) (F5,
// verbatim).
func ObservationDigest(consumed map[string]plan.ConsumedField) (string, error) {
	b, err := canonical.ToCanonicalJSON(ObservationDigestProjection(consumed))
	if err != nil {
		return "", err
	}
	return canonical.Digest(b), nil
}
