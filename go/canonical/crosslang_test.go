package canonical

import (
	"bufio"
	"bytes"
	"encoding/json"
	"os"
	"testing"
)

// The Rust peer's property suite (engine/tests/properties.rs,
// export_cross_language_corpus) writes a fixed-seed corpus of generated
// documents: strict JSON text, plus the canonical bytes and digest the Rust
// pipeline derived from it (reader -> to_canonical_json -> digest). This test
// re-derives both here and compares, so Rust and Go are checked against each
// other on generated input and not only on the hand-written differential
// vectors.
//
// CIC_PROP_CORPUS_IN names the corpus file. The gate sets it; when it is
// unset the test is skipped, so `go test ./...` still works on its own. When
// it is set the corpus must exist and be non-empty: a gate whose input
// silently vanished would pass for the wrong reason.
func TestCrossLanguageGeneratedCorpus(t *testing.T) {
	path := os.Getenv("CIC_PROP_CORPUS_IN")
	if path == "" {
		t.Skip("CIC_PROP_CORPUS_IN not set (the gate generates it with the Rust property suite)")
	}
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("corpus: %v", err)
	}
	defer f.Close()

	var n int
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 1<<20), 1<<26)
	for sc.Scan() {
		var c struct {
			Input     string `json:"input"`
			Canonical string `json:"canonical"`
			Digest    string `json:"digest"`
		}
		if err := json.Unmarshal(sc.Bytes(), &c); err != nil {
			t.Fatalf("line %d: %v", n+1, err)
		}
		n++

		dec := json.NewDecoder(bytes.NewReader([]byte(c.Input)))
		dec.UseNumber()
		var v interface{}
		if err := dec.Decode(&v); err != nil {
			t.Fatalf("line %d: Go cannot read the generated input: %v\n%s", n, err, c.Input)
		}
		got, err := ToCanonicalJSON(v)
		if err != nil {
			t.Fatalf("line %d: Go refuses what Rust canonicalized: %v\n%s", n, err, c.Input)
		}
		if string(got) != c.Canonical {
			t.Fatalf("line %d: canonical bytes differ\n input: %s\n  rust: %s\n    go: %s", n, c.Input, c.Canonical, got)
		}
		if d := Digest(got); d != c.Digest {
			t.Fatalf("line %d: digest differs: rust %s, go %s", n, c.Digest, d)
		}
	}
	if err := sc.Err(); err != nil {
		t.Fatal(err)
	}
	if n == 0 {
		t.Fatal("the corpus is empty")
	}
	t.Logf("%d generated documents agree", n)
}
