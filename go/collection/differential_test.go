package collection

// Runs the language-independent differential corpus in
// conformance/differential/ -- see that directory's own README, and
// go/conformance/differential_test.go's own doc comment, for why it is a
// second, JSON-only corpus layer. Mirrors engine/tests/differential.rs's
// own collection_vectors, vector-for-vector, against the identical JSON
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
	// (go/collection/), so this resolves to <repo-root>/conformance/differential/<group>.
	return filepath.Join("..", "..", "conformance", "differential", group)
}

type collectionVectorInput struct {
	Topology string      `json:"topology"`
	Keys     []string    `json:"keys"`
	Elem     interface{} `json:"elem"`
}

type collectionVectorExpected struct {
	Identity string `json:"identity"`
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

// Includes the exact negative-zero vector this package's own
// cross-language bugfix (collection.rs's go_float_display used to fold
// -0.0 to "0", found while building this Go peer, PR #32) was found and
// fixed against -- landing it here means that specific regression now
// also fails loudly for either language on its own, through the
// identical fixture, not only through each package's own hand-written
// unit test.
func TestDifferentialCollectionVectors(t *testing.T) {
	root := differentialCorpusRoot("collection")
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
		var in collectionVectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp collectionVectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		c := Collection{Topology: CollectionTopology(in.Topology), Keys: in.Keys}
		got := c.ElementKey(in.Elem)
		if got != exp.Identity {
			t.Errorf("%s: got %q, want %q", name, got, exp.Identity)
		}
	}
}
