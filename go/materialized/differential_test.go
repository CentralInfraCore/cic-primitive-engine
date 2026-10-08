package materialized

// Runs conformance/differential/materialized/ -- see that directory's
// own README, and engine/tests/differential.rs's own materialized_vectors,
// for why this is a second, JSON-only corpus layer and why it is
// deliberately narrower than F6-F9's own differential groups. Mirrors
// materialized_vectors, vector-for-vector, against the identical JSON
// fixtures.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

func differentialCorpusRoot(group string) string {
	// go test's working directory is this package's own source directory
	// (go/materialized/), so this resolves to <repo-root>/conformance/differential/<group>.
	return filepath.Join("..", "..", "conformance", "differential", group)
}

type materializedVectorInput struct {
	Fields       []string `json:"fields"`
	ExpectedKeys []string `json:"expected_keys"`
}

type materializedVectorExpected struct {
	Accepted bool `json:"accepted"`
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

// TestDifferentialMaterializedVectors is the Go peer of
// engine/tests/differential.rs's own materialized_vectors:
// NewMaterializedObject's Complete-property check -- given a
// candidate's own key set and the key set the environment claims the
// schema declares, both languages must agree on accept/reject.
//
// Field VALUES don't matter for this check (the key-set comparison
// never inspects them), so every field here is wrapped as a trivial
// Intent(Authored(true)) -- only the key NAMES are actually
// exercised.
func TestDifferentialMaterializedVectors(t *testing.T) {
	root := differentialCorpusRoot("materialized")
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
		var in materializedVectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp materializedVectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		fields := make(map[string]MaterializedField, len(in.Fields))
		for _, k := range in.Fields {
			fields[k] = MaterializedField{
				Capability: CapabilityImplemented,
				Evidence:   NewIntentFieldEvidence(NewAuthoredIntentEvidence(true, true)),
			}
		}
		expectedKeys := make(map[string]struct{}, len(in.ExpectedKeys))
		for _, k := range in.ExpectedKeys {
			expectedKeys[k] = struct{}{}
		}

		_, err := NewMaterializedObject(fields, expectedKeys)
		gotAccepted := err == nil
		if gotAccepted != exp.Accepted {
			t.Errorf("%s: accepted: got %v, want %v", name, gotAccepted, exp.Accepted)
		}
	}
}
