package canonical

import (
	"bytes"
	"encoding/json"
	"testing"
)

// FuzzToCanonicalJSON feeds arbitrary bytes through encoding/json (numbers
// kept as text, as the engine's callers do) and into ToCanonicalJSON. Without
// -fuzz the seeds below run as ordinary tests; the scheduled workflow runs the
// fuzzer. Whatever Go canonicalizes must be valid JSON and a fixed point of
// canonicalization -- and nothing may panic.
func FuzzToCanonicalJSON(f *testing.F) {
	for _, s := range []string{
		`null`, `true`, `0`, `-0`, `1e400`, `1E-400`, `9223372036854775808`, `-9223372036854775809`,
		`0.1`, `100000000000000000000.5`, `"  <>&"`, `"\u0000\u001f"`, `""`,
		`[]`, `{}`, `[[],{},[{}]]`, `{"b":1,"a":2,"":3}`, `{"a":{"c":1,"b":[1,2,{"z":0,"y":null}]}}`,
		`{"a":1,"a":2}`, `{"é":1,"é":2}`, `[1.0,1,1e0,1E0,0.5e1]`,
	} {
		f.Add([]byte(s))
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		dec := json.NewDecoder(bytes.NewReader(data))
		dec.UseNumber()
		var v interface{}
		if err := dec.Decode(&v); err != nil {
			return // not JSON: nothing to canonicalize
		}
		first, err := ToCanonicalJSON(v)
		if err != nil {
			return // a refusal (e.g. a number with no canonical form) is allowed; a panic is not
		}
		if !json.Valid(first) {
			t.Fatalf("canonical output is not valid JSON: %q (input %q)", first, data)
		}
		dec2 := json.NewDecoder(bytes.NewReader(first))
		dec2.UseNumber()
		var v2 interface{}
		if err := dec2.Decode(&v2); err != nil {
			t.Fatalf("canonical output does not decode: %v: %q", err, first)
		}
		second, err := ToCanonicalJSON(v2)
		if err != nil {
			t.Fatalf("canonical output is not canonicalizable again: %v: %q", err, first)
		}
		if !bytes.Equal(first, second) {
			t.Fatalf("canonicalization is not a fixed point:\n first: %q\nsecond: %q\n input: %q", first, second, data)
		}
	})
}
