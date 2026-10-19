package receipt

import (
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
