package conformance

// Runs the language-independent differential corpus in
// conformance/differential/ -- see that directory's own README for why it
// is a second, JSON-only corpus layer, distinct from ../reader/'s and
// ../canonicalize/'s YAML vectors: it exists to check whether the F6-F9
// primitives agree across languages given an already-parsed value, not
// whether the two languages' YAML readers agree with each other. Read
// directly with encoding/json -- no YAML reader exists on the Go side yet
// (that is Parse's own job, not yet built in either language), and this
// layer deliberately needs none, since valid JSON is already valid YAML
// and the Rust side keeps reading it with its existing reader::parse.

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

func differentialCorpusRoot(group string) string {
	// go test's working directory is this package's own source directory
	// (go/conformance/), so this resolves to <repo-root>/conformance/differential/<group>.
	return filepath.Join("..", "..", "conformance", "differential", group)
}

type comparatorVectorInput struct {
	Coverage      string      `json:"coverage"`
	IntentPresent bool        `json:"intent_present"`
	Intent        interface{} `json:"intent"`
	Observed      interface{} `json:"observed"`
	Compare       string      `json:"compare"`
}

type comparatorVectorExpected struct {
	Verdict string `json:"verdict"`
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

// Mirrors engine/tests/differential.rs's own comparator_vectors,
// vector-for-vector, against the identical JSON fixtures.
func TestDifferentialComparatorVectors(t *testing.T) {
	root := differentialCorpusRoot("comparator")
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

	seenVerdicts := map[string]bool{}
	for _, name := range names {
		dir := filepath.Join(root, name)
		var in comparatorVectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp comparatorVectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		got := ClassifyFieldValue(
			Coverage(in.Coverage),
			in.IntentPresent,
			in.Intent,
			in.Observed,
			CompareType(in.Compare),
		)
		if string(got) != exp.Verdict {
			t.Errorf("%s: got %s, want %s", name, got, exp.Verdict)
		}
		seenVerdicts[exp.Verdict] = true
	}

	// The harness's own invariant, mirroring differential.rs's identical
	// check: a corpus that never exercises a given verdict can't actually
	// prove either language produces it correctly.
	for _, v := range []string{"CONFORMANT", "DRIFT", "OBSERVED_ABSENT", "UNOBSERVED", "NOT_COMPARABLE"} {
		if !seenVerdicts[v] {
			t.Errorf("no vector in %s exercises verdict %s", root, v)
		}
	}
}
