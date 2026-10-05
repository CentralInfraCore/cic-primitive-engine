package digestprojection

// Runs the language-independent differential corpus in
// conformance/differential/ -- see that directory's own README, and
// go/conformance/differential_test.go's own doc comment, for why it is a
// second, JSON-only corpus layer. Mirrors engine/tests/differential.rs's
// own digest_vectors, vector-for-vector, against the identical JSON
// fixtures.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

func differentialCorpusRoot(group string) string {
	// go test's working directory is this package's own source directory
	// (go/digestprojection/), so this resolves to
	// <repo-root>/conformance/differential/<group>.
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

type digestVectorInput struct {
	Plan *struct {
		Scalars     []fieldPlanInput      `json:"scalars"`
		Collections []collectionPlanInput `json:"collections"`
	} `json:"plan"`
	Consumed []struct {
		Path     string      `json:"path"`
		Coverage string      `json:"coverage"`
		Value    interface{} `json:"value"`
	} `json:"consumed"`
}

type digestVectorExpected struct {
	Digest string `json:"digest"`
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

// Every expected digest in this corpus was computed once from the real
// ConformancePlanDigest/ObservationDigest functions in BOTH languages and
// confirmed byte-for-byte identical before being pinned -- not invented
// or hand-computed -- so a failure here means one language's output
// actually changed, not that the pinned value was ever a guess.
func TestDifferentialDigestVectors(t *testing.T) {
	root := differentialCorpusRoot("digest")
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
		var in digestVectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp digestVectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		var got string
		var err error
		switch {
		case in.Plan != nil:
			scalars := make([]plan.FieldPlan, len(in.Plan.Scalars))
			for i, fp := range in.Plan.Scalars {
				scalars[i] = plan.FieldPlan{Path: fp.Path, Compare: conformance.CompareType(fp.Compare)}
			}
			collections := make([]plan.CollectionPlan, len(in.Plan.Collections))
			for i, cp := range in.Plan.Collections {
				elements := make([]plan.FieldPlan, len(cp.Elements))
				for j, fp := range cp.Elements {
					elements[j] = plan.FieldPlan{Path: fp.Path, Compare: conformance.CompareType(fp.Compare)}
				}
				collections[i] = plan.CollectionPlan{
					Path: cp.Path,
					Collection: collection.Collection{
						Topology: collection.CollectionTopology(cp.Topology),
						Keys:     cp.Keys,
					},
					Elements: elements,
				}
			}
			got, err = ConformancePlanDigest(plan.ConformancePlan{Scalars: scalars, Collections: collections})
		case in.Consumed != nil:
			consumed := make(map[string]plan.ConsumedField, len(in.Consumed))
			for _, c := range in.Consumed {
				switch conformance.Coverage(c.Coverage) {
				case conformance.CoverageObserved:
					consumed[c.Path] = plan.NewObservedConsumedField(c.Value)
				case conformance.CoverageAbsent:
					consumed[c.Path] = plan.NewAbsentConsumedField()
				case conformance.CoverageUnobserved:
					consumed[c.Path] = plan.NewUnobservedConsumedField()
				case conformance.CoverageUnknown:
					consumed[c.Path] = plan.NewUnknownConsumedField()
				default:
					t.Fatalf("%s: unknown coverage %q", name, c.Coverage)
				}
			}
			got, err = ObservationDigest(consumed)
		default:
			t.Fatalf("%s: input.json has neither \"plan\" nor \"consumed\"", name)
		}
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if got != exp.Digest {
			t.Errorf("%s: got %s, want %s", name, got, exp.Digest)
		}
	}
}
