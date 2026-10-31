package schemaidentity

// Runs the schema_identity group of the language-independent differential corpus in conformance/differential/ -- the Go peer of
// engine/tests/differential.rs's schema_identity_vectors and schema_identity_vectors_detect_deliberate_divergence, reading the identical
// files, the schema source bytes included. A null in input.json is a nil pointer (the component or the claim is absent).

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"testing"
)

func corpusRoot() string {
	// go test runs in this package's directory (go/schemaidentity/).
	return filepath.Join("..", "..", "conformance", "differential", "schema_identity")
}

type vectorInput struct {
	Source        string  `json:"source"`
	ClaimedDigest *string `json:"claimed_digest"`
	Namespace     *string `json:"namespace"`
	Kind          *string `json:"kind"`
}

type vectorError struct {
	Code string `json:"code"`
	Rule string `json:"rule"`
	Path string `json:"path"`
}

type vectorExpected struct {
	Accepted     bool         `json:"accepted"`
	SchemaName   string       `json:"schema_name"`
	SchemaDigest string       `json:"schema_digest"`
	Error        *vectorError `json:"error"`
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

func vectorDirs(t *testing.T) []string {
	t.Helper()
	entries, err := os.ReadDir(corpusRoot())
	if err != nil {
		t.Fatal(err)
	}
	var dirs []string
	for _, e := range entries {
		if e.IsDir() {
			dirs = append(dirs, e.Name())
		}
	}
	sort.Strings(dirs)
	return dirs
}

func TestSchemaIdentityVectors(t *testing.T) {
	dirs := vectorDirs(t)
	if len(dirs) == 0 {
		t.Fatal("differential/schema_identity has no vectors -- an empty corpus trivially passes")
	}
	accepted, rejected := 0, 0
	for _, name := range dirs {
		var in vectorInput
		var want vectorExpected
		readJSON(t, filepath.Join(corpusRoot(), name, "input.json"), &in)
		readJSON(t, filepath.Join(corpusRoot(), name, "expected.json"), &want)
		source, err := os.ReadFile(filepath.Join(corpusRoot(), name, in.Source))
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		got, err := BindSchemaIdentity(source, in.ClaimedDigest, in.Namespace, in.Kind)
		if want.Accepted {
			accepted++
			if err != nil {
				t.Errorf("%s: want accepted, got %v", name, err)
			} else if got.SchemaName != want.SchemaName || got.SchemaDigest != want.SchemaDigest {
				t.Errorf("%s: want (%q, %q), got (%q, %q)", name, want.SchemaName, want.SchemaDigest, got.SchemaName, got.SchemaDigest)
			}
			continue
		}
		rejected++
		e, ok := err.(*Error)
		if !ok {
			t.Errorf("%s: want rejection %+v, got %v", name, *want.Error, err)
			continue
		}
		if e.Code != want.Error.Code || e.Rule != want.Error.Rule || e.Path != want.Error.Path {
			t.Errorf("%s: want %+v, got {%s %s %s}", name, *want.Error, e.Code, e.Rule, e.Path)
		}
	}
	if accepted < 5 || rejected < 18 {
		t.Errorf("corpus too thin: %d accepted, %d rejected", accepted, rejected)
	}
}

func TestSchemaIdentityVectorsDetectDeliberateDivergence(t *testing.T) {
	source, err := os.ReadFile(filepath.Join(corpusRoot(), "golden_with_final_lf", "schema.yaml"))
	if err != nil {
		t.Fatal(err)
	}
	if len(source) != 314 {
		t.Fatalf("the golden source is %d bytes, want 314", len(source))
	}
	golden := SchemaDigest(source)
	ns, kind := "cic:storage", "K"
	for i := range source {
		altered := append([]byte(nil), source...)
		altered[i] ^= 1
		if SchemaDigest(altered) == golden {
			t.Fatalf("byte %d: the digest does not depend on it", i)
		}
		_, err := BindSchemaIdentity(altered, &golden, &ns, &kind)
		if e, ok := err.(*Error); !ok || e.Rule != "R-SCHEMA-DIGEST-MISMATCH" {
			t.Fatalf("byte %d: want the mismatch rule, got %v", i, err)
		}
	}
	if SchemaDigest(source[:313]) == golden {
		t.Fatal("the digest ignores the final LF")
	}
}

func TestSchemaDigestKnownAnswers(t *testing.T) {
	// FIPS 180-4 examples, independent of the corpus
	if got := SchemaDigest(nil); got != "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855" {
		t.Errorf("empty: %s", got)
	}
	if got := SchemaDigest([]byte("abc")); got != "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad" {
		t.Errorf("abc: %s", got)
	}
}
