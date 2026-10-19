package receipt

// Runs the receipt group of the language-independent differential corpus in
// conformance/differential/ -- the Go peer of engine/tests/differential.rs's
// receipt_vectors and receipt_vectors_detect_deliberate_divergence, reading
// the identical JSON files. See that directory's README for why the corpus
// is JSON-only.
//
// Documents are decoded with UseNumber so an integer stays an integer and
// 1.0 stays a non-integer, as the Rust reader reads them.

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
)

func corpusRoot(group string) string {
	// go test runs in this package's directory (go/receipt/).
	return filepath.Join("..", "..", "conformance", "differential", group)
}

func decodeFile(t *testing.T, path string) map[string]interface{} {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("%s: %v", path, err)
	}
	dec := json.NewDecoder(bytes.NewReader(b))
	dec.UseNumber()
	var v map[string]interface{}
	if err := dec.Decode(&v); err != nil {
		t.Fatalf("%s: %v", path, err)
	}
	return v
}

// objectFromFixture builds a MaterializedObject from a digest-group style
// {"fields": [{"key", "capability", "coverage"?, "provenance"?, "value"?}]}
// fixture -- the identical shape engine/tests/differential.rs's
// build_materialized_object_from_fixture reads.
func objectFromFixture(t *testing.T, name string, fixture interface{}) materialized.MaterializedObject {
	t.Helper()
	items := fixture.(map[string]interface{})["fields"].([]interface{})
	out := make(map[string]materialized.MaterializedField, len(items))
	keys := make(map[string]struct{}, len(items))
	for _, raw := range items {
		f := raw.(map[string]interface{})
		key, _ := f["key"].(string)
		capability, _ := f["capability"].(string)
		coverageRaw, hasCoverage := f["coverage"]
		provenanceRaw, hasProvenance := f["provenance"]
		valueRaw, hasValue := f["value"]

		var evidence materialized.FieldEvidence
		switch {
		case hasCoverage && hasProvenance:
			cov, _ := coverageRaw.(string)
			evidence = materialized.NewDerivedObservationFieldEvidence(conformance.Coverage(cov), valueRaw)
		case hasCoverage:
			cov, _ := coverageRaw.(string)
			evidence = materialized.NewObservationFieldEvidence(conformance.Coverage(cov), valueRaw)
		case hasProvenance:
			prov, _ := provenanceRaw.(string)
			switch prov {
			case "authored":
				evidence = materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(valueRaw, hasValue))
			case "schema_default":
				evidence = materialized.NewIntentFieldEvidence(materialized.NewSchemaDefaultIntentEvidence(valueRaw))
			case "derived":
				evidence = materialized.NewIntentFieldEvidence(materialized.NewDerivedIntentEvidence(valueRaw))
			default:
				t.Fatalf("%s: field %q: unknown provenance %q", name, key, prov)
			}
		default:
			t.Fatalf("%s: field %q: neither coverage nor provenance", name, key)
		}
		out[key] = materialized.MaterializedField{Capability: materialized.Capability(capability), Evidence: evidence}
		keys[key] = struct{}{}
	}
	obj, err := materialized.NewMaterializedObject(out, keys)
	if err != nil {
		t.Fatalf("%s: fixture object: %v", name, err)
	}
	return obj
}

type outcome struct{ code, rule, path string }

// runVector returns nil for an accepted vector, else the rejection.
func runVector(t *testing.T, name string, input map[string]interface{}) *outcome {
	t.Helper()
	obj := objectFromFixture(t, name, input["object"])
	binding := PathBinding{}
	if raw, ok := input["path_binding"].(map[string]interface{}); ok {
		for k, v := range raw {
			binding[k] = v.(string)
		}
	}
	r, err := Parse(input["receipt"])
	if err == nil {
		err = Verify(r, obj, input["authored_input"], binding)
	}
	if err == nil {
		return nil
	}
	var e *Error
	if !errors.As(err, &e) {
		t.Fatalf("%s: rejection is not a *receipt.Error: %v", name, err)
	}
	return &outcome{e.Code, e.Rule, e.Path}
}

func vectorNames(t *testing.T) []string {
	t.Helper()
	entries, err := os.ReadDir(corpusRoot("receipt"))
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range entries {
		if e.IsDir() {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)
	return names
}

func TestDifferentialReceiptVectors(t *testing.T) {
	names := vectorNames(t)
	if len(names) == 0 {
		t.Fatal("differential/receipt has no vectors -- an empty corpus trivially passes")
	}
	accepted, rejected := 0, 0
	for _, name := range names {
		dir := filepath.Join(corpusRoot("receipt"), name)
		input := decodeFile(t, filepath.Join(dir, "input.json"))
		expected := decodeFile(t, filepath.Join(dir, "expected.json"))
		got := runVector(t, name, input)
		if expected["accepted"].(bool) {
			accepted++
			if got != nil {
				t.Errorf("%s: want accepted, got %+v", name, *got)
			}
			continue
		}
		rejected++
		w := expected["error"].(map[string]interface{})
		want := outcome{w["code"].(string), w["rule"].(string), w["path"].(string)}
		switch {
		case got == nil:
			t.Errorf("%s: want rejection %+v, got accepted", name, want)
		case *got != want:
			t.Errorf("%s: want %+v, got %+v", name, want, *got)
		}
	}
	if accepted < 3 || rejected < 20 {
		t.Fatalf("corpus too thin: %d accepted, %d rejected", accepted, rejected)
	}
}

// The harness must be able to fail: every accepted vector, with one field of
// its receipt altered, must turn into the rejection that alteration deserves.
func TestDifferentialReceiptVectorsDetectDeliberateDivergence(t *testing.T) {
	flipHex := func(d string) string {
		b := []byte(d)
		if b[len(b)-1] == '0' {
			b[len(b)-1] = '1'
		} else {
			b[len(b)-1] = '0'
		}
		return string(b)
	}
	mutations := []struct {
		what, code string
		mutate     func(m map[string]interface{})
	}{
		{"input digest altered", CodeReceiptMismatch, func(m map[string]interface{}) {
			m["input_digest"] = flipHex(m["input_digest"].(string))
		}},
		{"output digest altered", CodeReceiptMismatch, func(m map[string]interface{}) {
			m["output_digest"] = flipHex(m["output_digest"].(string))
		}},
		{"version altered", CodeInvalidReceipt, func(m map[string]interface{}) {
			m["receipt_schema_version"] = json.Number("2")
		}},
		{"field retyped", CodeInvalidReceipt, func(m map[string]interface{}) {
			m["engine_version"] = json.Number("1")
		}},
	}
	checked := 0
	for _, name := range vectorNames(t) {
		dir := filepath.Join(corpusRoot("receipt"), name)
		if !decodeFile(t, filepath.Join(dir, "expected.json"))["accepted"].(bool) {
			continue
		}
		if got := runVector(t, name, decodeFile(t, filepath.Join(dir, "input.json"))); got != nil {
			t.Fatalf("%s: baseline rejected: %+v", name, *got)
		}
		for _, m := range mutations {
			input := decodeFile(t, filepath.Join(dir, "input.json"))
			body := input["receipt"].(map[string]interface{})["materialization"].(map[string]interface{})
			m.mutate(body)
			got := runVector(t, name, input)
			if got == nil || got.code != m.code {
				t.Fatalf("%s: %s: expected %s, got %v", name, m.what, m.code, got)
			}
			checked++
		}
	}
	if checked < 12 {
		t.Fatalf("too few mutations were exercised: %d", checked)
	}
}
