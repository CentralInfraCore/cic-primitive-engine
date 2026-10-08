package plan

// Runs the language-independent differential corpus in
// conformance/differential/ -- see that directory's own README, and
// go/conformance/differential_test.go's own doc comment, for why it is a
// second, JSON-only corpus layer. Mirrors engine/tests/differential.rs's
// own plan_vectors, vector-for-vector, against the identical JSON
// fixtures.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
)

func differentialCorpusRoot(group string) string {
	// go test's working directory is this package's own source directory
	// (go/plan/), so this resolves to <repo-root>/conformance/differential/<group>.
	return filepath.Join("..", "..", "conformance", "differential", group)
}

type fieldPlanInput struct {
	Path    string `json:"path"`
	Compare string `json:"compare"`
}

type collectionPlanInput struct {
	Path     string           `json:"path"`
	Topology string           `json:"topology"`
	Keys     []string         `json:"keys"`
	Elements []fieldPlanInput `json:"elements"`
}

type planVectorInput struct {
	Intent      map[string]interface{} `json:"intent"`
	Observed    map[string]interface{} `json:"observed"`
	Observation []struct {
		Path     string `json:"path"`
		Coverage string `json:"coverage"`
	} `json:"observation"`
	Plan struct {
		Scalars     []fieldPlanInput      `json:"scalars"`
		Collections []collectionPlanInput `json:"collections"`
	} `json:"plan"`
}

type planVectorExpected struct {
	Object string            `json:"object"`
	Fields map[string]string `json:"fields"`
}

func readJSON(t *testing.T, path string, v interface{}) {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("%s: %v", path, err)
	}
	if err := json.Unmarshal(b, v); err != nil {
		t.Fatalf("%s: %v", path, err)
	}
}

func toFieldPlan(fp fieldPlanInput) FieldPlan {
	return FieldPlan{Path: fp.Path, Compare: conformance.CompareType(fp.Compare)}
}

// Includes the exact multi-key-identity vector F8's own review round
// found and fixed (resolvePath's inherited Relay bug, PR #28) -- intent
// and observed deliberately differ, so a silently-failed path resolution
// (both sides falling back to the same missing-path default) would
// produce a false CONFORMANT instead of the DRIFT this vector actually
// requires.
func TestDifferentialPlanVectors(t *testing.T) {
	root := differentialCorpusRoot("plan")
	entries, err := os.ReadDir(root)
	if err != nil {
		t.Fatalf("%s: %v", root, err)
	}
	var names []string
	for _, e := range entries {
		if e.IsDir() {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)
	if len(names) == 0 {
		t.Fatalf("%s: no vectors -- an empty corpus trivially passes", root)
	}

	for _, name := range names {
		dir := filepath.Join(root, name)
		var in planVectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp planVectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		obs := conformance.NewObservation()
		for _, o := range in.Observation {
			obs.Set(o.Path, conformance.Coverage(o.Coverage))
		}

		scalars := make([]FieldPlan, len(in.Plan.Scalars))
		for i, fp := range in.Plan.Scalars {
			scalars[i] = toFieldPlan(fp)
		}
		collections := make([]CollectionPlan, len(in.Plan.Collections))
		for i, cp := range in.Plan.Collections {
			elements := make([]FieldPlan, len(cp.Elements))
			for j, fp := range cp.Elements {
				elements[j] = toFieldPlan(fp)
			}
			collections[i] = CollectionPlan{
				Path: cp.Path,
				Collection: collection.Collection{
					Topology: collection.CollectionTopology(cp.Topology),
					Keys:     cp.Keys,
				},
				Elements: elements,
			}
		}
		cp := ConformancePlan{Scalars: scalars, Collections: collections}

		verdict := Evaluate(intentObject(t, in.Intent), observedObject(t, in.Observed, obs), obs, cp)

		if string(verdict.Object) != exp.Object {
			t.Errorf("%s: object: got %s, want %s", name, verdict.Object, exp.Object)
		}

		gotFields := make(map[string]string, len(verdict.Fields))
		for path, fv := range verdict.Fields {
			gotFields[path] = string(fv)
		}
		if len(gotFields) != len(exp.Fields) {
			t.Errorf("%s: fields: got %d entries, want %d (got=%v want=%v)", name, len(gotFields), len(exp.Fields), gotFields, exp.Fields)
			continue
		}
		for path, want := range exp.Fields {
			if got := gotFields[path]; got != want {
				t.Errorf("%s: field %s: got %s, want %s", name, path, got, want)
			}
		}
	}
}
