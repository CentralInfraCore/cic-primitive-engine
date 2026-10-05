package plan

import (
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
)

func nic(name, subnetRef string, extra map[string]interface{}) map[string]interface{} {
	m := map[string]interface{}{
		"name": name,
		"subnet": map[string]interface{}{
			"ref":  subnetRef,
			"kind": "Subnet",
		},
	}
	for k, v := range extra {
		m[k] = v
	}
	return m
}

// Mirrors engine/src/plan.rs's oci_intent/oci_observed/oci_plan/
// oci_observation fixtures -- the OCI vertical slice, ported field for
// field, not invented.
func ociIntent() map[string]interface{} {
	return map[string]interface{}{
		"shape":     "VM.Standard.E4.Flex",
		"memory_gb": 16,
		"network_interfaces": []interface{}{
			nic("nic-0", "prod-subnet-a", nil),
			nic("nic-1", "prod-subnet-b", nil),
		},
	}
}

func ociObserved() map[string]interface{} {
	return map[string]interface{}{
		"shape":           "VM.Standard.E4.Flex",
		"memory_gb":       16.0, // numeric-equal to 16, not exact-equal
		"provider_id":     "ocid1.instance.oc1..aaaa",
		"lifecycle_state": "RUNNING",
		"network_interfaces": []interface{}{
			nic("nic-0", "prod-subnet-a", map[string]interface{}{"private_ip": "10.0.1.17"}),
			nic("nic-1", "prod-subnet-b", map[string]interface{}{"private_ip": "10.0.2.9"}),
		},
	}
}

func ociPlan() ConformancePlan {
	return ConformancePlan{
		Scalars: []FieldPlan{
			{Path: "/shape", Compare: conformance.CompareExact},
			{Path: "/memory_gb", Compare: conformance.CompareNumeric},
		},
		Collections: []CollectionPlan{
			{
				Path: "/network_interfaces",
				Collection: collection.Collection{
					Topology: collection.TopologyMap,
					Keys:     []string{"name"},
				},
				Elements: []FieldPlan{
					{Path: "subnet", Compare: conformance.CompareExact},
				},
			},
		},
	}
}

func ociObservation() *conformance.Observation {
	obs := conformance.NewObservation()
	for _, p := range []string{
		"/shape",
		"/memory_gb",
		"/network_interfaces/{name=nic-0}/subnet",
		"/network_interfaces/{name=nic-1}/subnet",
	} {
		obs.Set(p, conformance.CoverageObserved)
	}
	return obs
}

// Mirrors plan.rs's oci_conformant, which itself mirrors
// TestEvaluate_OCI_Conformant.
func TestOCIConformant(t *testing.T) {
	verdict := Evaluate(ociIntent(), ociObserved(), ociObservation(), ociPlan())
	if verdict.Object != ObjectConformant {
		t.Fatalf("object: got %v, want CONFORMANT", verdict.Object)
	}
	for path, v := range verdict.Fields {
		if v != conformance.VerdictConformant {
			t.Errorf("%s: got %v, want CONFORMANT", path, v)
		}
	}
	// numeric normalization actually exercised: int(16) vs float64(16.0).
	if got := verdict.Fields["/memory_gb"]; got != conformance.VerdictConformant {
		t.Errorf("/memory_gb: got %v, want CONFORMANT", got)
	}
	// map-collection element paths resolved by identity.
	if _, ok := verdict.Fields["/network_interfaces/{name=nic-1}/subnet"]; !ok {
		t.Error("expected /network_interfaces/{name=nic-1}/subnet in fields")
	}
}

// Mirrors plan.rs's oci_extra_state_fields_are_not_drift, which itself
// mirrors TestEvaluate_OCI_ExtraStateFieldsAreNotDrift.
func TestOCIExtraStateFieldsAreNotDrift(t *testing.T) {
	verdict := Evaluate(ociIntent(), ociObserved(), ociObservation(), ociPlan())
	for _, stateOnly := range []string{
		"/provider_id",
		"/lifecycle_state",
		"/network_interfaces/{name=nic-0}/private_ip",
	} {
		if _, ok := verdict.Fields[stateOnly]; ok {
			t.Errorf("state-only field %s leaked into the verdict set", stateOnly)
		}
	}
	if verdict.Object != ObjectConformant {
		t.Fatalf("object: got %v, want CONFORMANT", verdict.Object)
	}
}

// Mirrors plan.rs's oci_drift, which itself mirrors TestEvaluate_OCI_Drift.
func TestOCIDrift(t *testing.T) {
	observed := ociObserved()
	nics := observed["network_interfaces"].([]interface{})
	nics[1] = nic("nic-1", "prod-subnet-WRONG", nil)

	verdict := Evaluate(ociIntent(), observed, ociObservation(), ociPlan())
	if got := verdict.Fields["/network_interfaces/{name=nic-1}/subnet"]; got != conformance.VerdictDrift {
		t.Errorf("got %v, want DRIFT", got)
	}
	if verdict.Object != ObjectDrift {
		t.Fatalf("object: got %v, want DRIFT", verdict.Object)
	}
}

// Mirrors plan.rs's oci_not_comparable, which itself mirrors
// TestEvaluate_OCI_NotComparable.
func TestOCINotComparable(t *testing.T) {
	observed := ociObserved()
	observed["memory_gb"] = "large"

	verdict := Evaluate(ociIntent(), observed, ociObservation(), ociPlan())
	if got := verdict.Fields["/memory_gb"]; got != conformance.VerdictNotComparable {
		t.Errorf("got %v, want NOT_COMPARABLE", got)
	}
	if verdict.Object != ObjectDrift {
		t.Fatalf("object: got %v, want DRIFT", verdict.Object)
	}
}

// Mirrors plan.rs's oci_unobserved, which itself mirrors
// TestEvaluate_OCI_Unobserved.
func TestOCIUnobserved(t *testing.T) {
	obs := conformance.NewObservation()
	for _, p := range []string{
		"/shape",
		"/network_interfaces/{name=nic-0}/subnet",
		"/network_interfaces/{name=nic-1}/subnet",
	} {
		obs.Set(p, conformance.CoverageObserved)
	}
	verdict := Evaluate(ociIntent(), ociObserved(), obs, ociPlan())
	if got := verdict.Fields["/memory_gb"]; got != conformance.VerdictUnobserved {
		t.Errorf("got %v, want UNOBSERVED", got)
	}
	if verdict.Object != ObjectIncomplete {
		t.Fatalf("object: got %v, want INCOMPLETE", verdict.Object)
	}
}

// Mirrors plan.rs's oci_desired_absent_is_conformant, which itself
// mirrors TestEvaluate_OCI_DesiredAbsentIsConformant.
func TestOCIDesiredAbsentIsConformant(t *testing.T) {
	p := ociPlan()
	p.Scalars = append(p.Scalars, FieldPlan{Path: "/boot_volume", Compare: conformance.CompareExact})
	obs := ociObservation()
	obs.Set("/boot_volume", conformance.CoverageAbsent)

	verdict := Evaluate(ociIntent(), ociObserved(), obs, p)
	if got := verdict.Fields["/boot_volume"]; got != conformance.VerdictObservedAbsent {
		t.Errorf("got %v, want OBSERVED_ABSENT", got)
	}
	if verdict.Object != ObjectConformant {
		t.Fatalf("object: got %v, want CONFORMANT", verdict.Object)
	}
}

// Not in conformance_test.go -- this engine's own B2/B7-motivated
// addition, mirroring plan.rs's unknown_coverage_flows_through_to_unobserved_verdict:
// unknown coverage (no Relay equivalent) must flow through Evaluate
// the same way conformance.ClassifyFieldValue (F6/F11) already
// decided it does, not get lost or mishandled at the walker layer.
func TestUnknownCoverageFlowsThroughToUnobservedVerdict(t *testing.T) {
	obs := ociObservation()
	obs.Set("/memory_gb", conformance.CoverageUnknown)
	verdict := Evaluate(ociIntent(), ociObserved(), obs, ociPlan())
	if got := verdict.Fields["/memory_gb"]; got != conformance.VerdictUnobserved {
		t.Errorf("got %v, want UNOBSERVED", got)
	}
}

// Mirrors plan.rs's multi_key_collection_identity_resolves_back_to_its_element,
// proving resolvePath's multi-key fix (see the package doc) actually
// resolves a path Evaluate generated from its own multi-key
// ElementKey -- not just that the two sides happen to compare equal by
// both falling back to nil. Intent and observed deliberately differ,
// the same point plan.rs's own test makes: a silently-failed
// resolution would have both sides fall back to nil, which compares
// equal to itself, hiding the exact failure this test exists to catch.
func TestMultiKeyCollectionIdentityResolvesBackToItsElement(t *testing.T) {
	nicWithSubnet := func(subnet string) map[string]interface{} {
		return map[string]interface{}{
			"name":   "nic-0",
			"zone":   "eu",
			"subnet": subnet,
		}
	}
	p := ConformancePlan{
		Collections: []CollectionPlan{
			{
				Path: "/nics",
				Collection: collection.Collection{
					Topology: collection.TopologyMap,
					Keys:     []string{"name", "zone"},
				},
				Elements: []FieldPlan{
					{Path: "subnet", Compare: conformance.CompareExact},
				},
			},
		},
	}
	intent := map[string]interface{}{"nics": []interface{}{nicWithSubnet("prod-a")}}
	observed := map[string]interface{}{"nics": []interface{}{nicWithSubnet("prod-WRONG")}}
	obs := conformance.NewObservation()
	obs.Set("/nics/{name=nic-0,zone=eu}/subnet", conformance.CoverageObserved)

	verdict := Evaluate(intent, observed, obs, p)
	got := verdict.Fields["/nics/{name=nic-0,zone=eu}/subnet"]
	if got != conformance.VerdictDrift {
		t.Fatalf(
			"a multi-key identity must resolve back to the SAME real element on both sides, "+
				"not silently fall back to nil on both -- got %v, fields=%v", got, verdict.Fields,
		)
	}
	if verdict.Object != ObjectDrift {
		t.Fatalf("object: got %v, want DRIFT", verdict.Object)
	}
}

func expectPanic(t *testing.T, name string, fn func()) {
	t.Helper()
	defer func() {
		if recover() == nil {
			t.Errorf("%s: expected a panic, got none", name)
		}
	}()
	fn()
}

// Applying PR #31/#32's review lesson proactively here too: Evaluate
// validates intent/observed against canonical.IsValue at its own
// boundary, before resolvePath/ElementKey ever touch them downstream.
type notAValue struct{ X int }

func TestEvaluateFailsClosedOnOutOfDomainTrees(t *testing.T) {
	expectPanic(t, "out-of-domain nested in intent", func() {
		Evaluate(
			map[string]interface{}{"shape": notAValue{1}},
			ociObserved(),
			ociObservation(),
			ociPlan(),
		)
	})
	expectPanic(t, "out-of-domain nested in observed", func() {
		Evaluate(
			ociIntent(),
			map[string]interface{}{"shape": notAValue{1}},
			ociObservation(),
			ociPlan(),
		)
	})
}

// NewConsumedField is this package's own closed-construction type
// (see ConsumedField's doc comment) -- Coverage must still be one of
// the four defined values, the same boundary check conformance's own
// ClassifyField/Compare/Observation.Set already apply.
func TestNewConsumedFieldFailsClosedOnInvalidCoverage(t *testing.T) {
	expectPanic(t, "NewConsumedField", func() {
		NewConsumedField(conformance.Coverage("garbage"), "x")
	})
}

func TestConsumedFieldOnlyCarriesAValueWhenObserved(t *testing.T) {
	observed := NewConsumedField(conformance.CoverageObserved, "x")
	if v, ok := observed.Value(); !ok || v != "x" {
		t.Errorf("observed: got (%v, %v), want (\"x\", true)", v, ok)
	}
	for _, c := range []conformance.Coverage{
		conformance.CoverageAbsent, conformance.CoverageUnobserved, conformance.CoverageUnknown,
	} {
		cf := NewConsumedField(c, "should be discarded")
		if v, ok := cf.Value(); ok || v != nil {
			t.Errorf("%v: got (%v, %v), want (nil, false)", c, v, ok)
		}
		if cf.Coverage() != c {
			t.Errorf("Coverage() = %v, want %v", cf.Coverage(), c)
		}
	}
}
