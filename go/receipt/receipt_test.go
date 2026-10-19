package receipt

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/digestprojection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
)

var zero = "sha256:" + strings.Repeat("0", 64)

func mustObject(t *testing.T, fields map[string]materialized.FieldEvidence) materialized.MaterializedObject {
	t.Helper()
	m := map[string]materialized.MaterializedField{}
	keys := map[string]struct{}{}
	for k, ev := range fields {
		m[k] = materialized.MaterializedField{Capability: materialized.CapabilityImplemented, Evidence: ev}
		keys[k] = struct{}{}
	}
	obj, err := materialized.NewMaterializedObject(m, keys)
	if err != nil {
		t.Fatal(err)
	}
	return obj
}

func digestOf(t *testing.T, v interface{}) string {
	t.Helper()
	b, err := canonical.ToCanonicalJSON(v)
	if err != nil {
		t.Fatal(err)
	}
	return canonical.Digest(b)
}

func baseReceipt(t *testing.T, input interface{}, obj materialized.MaterializedObject) Receipt {
	t.Helper()
	out, err := digestprojection.MaterializedObjectDigest(obj)
	if err != nil {
		t.Fatal(err)
	}
	return Receipt{
		ReceiptSchemaVersion: 1, GrammarSHA256: zero, GrammarSchemaSHA256: zero,
		PrimitiveRelease: "primitives/@v0.2.0", PrimitiveSourceCommit: "960ee0f", PrimitiveBuildHash: zero,
		SchemaName: "cic:storage:StorageResource", SchemaVersion: "v1.2.0", SchemaDigest: zero,
		EngineIdentity: "environment-under-test", EngineVersion: "0.0.0",
		InputDigest: digestOf(t, input), OutputDigest: out,
		AppliedDefaults: []DefaultEvidence{}, DerivedValues: []DerivedEvidence{},
	}
}

func code(err error) string {
	var e *Error
	if errors.As(err, &e) {
		return e.Code
	}
	return ""
}

func authoredShape() materialized.FieldEvidence {
	return materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence("E4.Flex", true))
}

func TestEmptyEvidenceOverAnObjectWithoutDefaultsVerifiesWithoutABinding(t *testing.T) {
	input := map[string]interface{}{"shape": "E4.Flex"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{"shape": authoredShape()})
	if err := Verify(baseReceipt(t, input, obj), obj, input, nil); err != nil {
		t.Fatal(err)
	}
}

func TestADefaultedFieldTheReceiptDoesNotListIsRefused(t *testing.T) {
	input := map[string]interface{}{"shape": "E4.Flex"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{
		"replicas": materialized.NewIntentFieldEvidence(materialized.NewSchemaDefaultIntentEvidence(int64(3))),
	})
	err := Verify(baseReceipt(t, input, obj), obj, input, nil)
	var e *Error
	if !errors.As(err, &e) || e.Code != CodeReceiptMismatch || e.Path != "object/replicas" {
		t.Fatalf("got %v", err)
	}
}

func TestEvidenceWithoutABindingFailsClosed(t *testing.T) {
	input := map[string]interface{}{"shape": "E4.Flex"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{
		"replicas": materialized.NewIntentFieldEvidence(materialized.NewSchemaDefaultIntentEvidence(int64(3))),
	})
	r := baseReceipt(t, input, obj)
	r.AppliedDefaults = []DefaultEvidence{{Path: "$.config.replicas", Rule: DefaultRule, ValueDigest: digestOf(t, int64(3))}}
	if got := code(Verify(r, obj, input, nil)); got != CodeReceiptUnbound {
		t.Fatalf("no binding: got %q", got)
	}
	if err := Verify(r, obj, input, PathBinding{"$.config.replicas": "replicas"}); err != nil {
		t.Fatal(err)
	}
}

func TestDerivedInputsAreNotRequiredToExistInTheObject(t *testing.T) {
	input := map[string]interface{}{"admin_state": "up"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{
		"effective_state": materialized.NewIntentFieldEvidence(materialized.NewDerivedIntentEvidence("RUNNING")),
	})
	r := baseReceipt(t, input, obj)
	r.DerivedValues = []DerivedEvidence{{
		Path: "$.state.effective_state", Rule: "effective-state-v1",
		Inputs:      []string{"$.state.admin_state", "$.state.oper_state"},
		ValueDigest: digestOf(t, "RUNNING"),
	}}
	if err := Verify(r, obj, input, PathBinding{"$.state.effective_state": "effective_state"}); err != nil {
		t.Fatal(err)
	}
}

func TestADerivedObservationCountsAsDerived(t *testing.T) {
	input := map[string]interface{}{}
	obj := mustObject(t, map[string]materialized.FieldEvidence{
		"c": materialized.NewDerivedObservationFieldEvidence(conformance.CoverageObserved, "RUNNING"),
	})
	r := baseReceipt(t, input, obj)
	if got := code(Verify(r, obj, input, nil)); got != CodeReceiptMismatch {
		t.Fatalf("unlisted derived observation: got %q", got)
	}
}

func TestDigestFormIsExactlySha256And64LowercaseHex(t *testing.T) {
	for _, ok := range []string{zero} {
		if !isDigest(ok) {
			t.Errorf("%q should be a digest", ok)
		}
	}
	for _, bad := range []string{
		strings.ToUpper(zero), zero[7:], "sha256:00", zero + "0", "sha512:" + zero[7:], "",
	} {
		if isDigest(bad) {
			t.Errorf("%q should not be a digest", bad)
		}
	}
}

func TestPathAndRuleForms(t *testing.T) {
	if checkPathForm("$.a.b", "x") != nil {
		t.Error("$.a.b should be a path")
	}
	for _, bad := range []string{"$", "$.", "a.b", "$.a..b", "$.a."} {
		if checkPathForm(bad, "x") == nil {
			t.Errorf("%q should not be a path", bad)
		}
	}
	if checkDerivedRule("effective-state-v1", "x") != nil {
		t.Error("effective-state-v1 should be a rule")
	}
	for _, bad := range []string{"effective-state", "-v1", "x-v", "x-v1a", "x-vv1"} {
		if checkDerivedRule(bad, "x") == nil {
			t.Errorf("%q should not be a rule", bad)
		}
	}
}

// The evidence lists are required arrays, never null: a Go nil slice is the
// absent/null spelling, [] is the empty one (RECEIPT-SCHEMA.md).
func TestNilEvidenceListsAreRefusedOnADirectlyBuiltReceipt(t *testing.T) {
	input := map[string]interface{}{"shape": "E4.Flex"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{"shape": authoredShape()})

	for _, c := range []struct {
		name, path string
		mutate     func(r *Receipt)
	}{
		{"nil applied_defaults", "$.materialization.applied_defaults", func(r *Receipt) { r.AppliedDefaults = nil }},
		{"nil derived_values", "$.materialization.derived_values", func(r *Receipt) { r.DerivedValues = nil }},
		{"nil inputs", "$.materialization.derived_values[0].inputs", func(r *Receipt) {
			r.DerivedValues = []DerivedEvidence{{Path: "$.a", Rule: "x-v1", Inputs: nil, ValueDigest: zero}}
		}},
	} {
		r := baseReceipt(t, input, obj)
		c.mutate(&r)
		var e *Error
		for _, err := range []error{r.CheckStructure(), Verify(r, obj, input, nil)} {
			if !errors.As(err, &e) || e.Code != CodeInvalidReceipt || e.Rule != "R-RECEIPT-SHAPE" || e.Path != c.path {
				t.Errorf("%s: got %v", c.name, err)
			}
		}
	}
}

func TestEmptyJSONListsParseToNonNilSlicesAndVerify(t *testing.T) {
	input := map[string]interface{}{"shape": "E4.Flex"}
	obj := mustObject(t, map[string]materialized.FieldEvidence{"shape": authoredShape()})
	built := baseReceipt(t, input, obj)

	doc := map[string]interface{}{"materialization": map[string]interface{}{
		"receipt_schema_version":  json.Number("1"),
		"grammar_sha256":          built.GrammarSHA256,
		"grammar_schema_sha256":   built.GrammarSchemaSHA256,
		"primitive_release":       built.PrimitiveRelease,
		"primitive_source_commit": built.PrimitiveSourceCommit,
		"primitive_build_hash":    built.PrimitiveBuildHash,
		"schema_name":             built.SchemaName,
		"schema_version":          built.SchemaVersion,
		"schema_digest":           built.SchemaDigest,
		"engine_identity":         built.EngineIdentity,
		"engine_version":          built.EngineVersion,
		"input_digest":            built.InputDigest,
		"output_digest":           built.OutputDigest,
		"applied_defaults":        []interface{}{},
		"derived_values":          []interface{}{},
	}}
	r, err := Parse(doc)
	if err != nil {
		t.Fatal(err)
	}
	if r.AppliedDefaults == nil || r.DerivedValues == nil {
		t.Fatalf("[] must parse to non-nil slices: %#v %#v", r.AppliedDefaults, r.DerivedValues)
	}
	if err := Verify(r, obj, input, nil); err != nil {
		t.Fatal(err)
	}

	// An empty inputs list inside a derived entry is non-nil too.
	doc["materialization"].(map[string]interface{})["derived_values"] = []interface{}{
		map[string]interface{}{"path": "$.a", "rule": "x-v1", "inputs": []interface{}{}, "value_digest": zero},
	}
	r, err = Parse(doc)
	if err != nil {
		t.Fatal(err)
	}
	if r.DerivedValues[0].Inputs == nil {
		t.Fatal("[] inputs must parse to a non-nil slice")
	}
}
