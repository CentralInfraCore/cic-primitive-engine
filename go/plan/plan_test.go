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

// Review-caught on PR #33: a malformed ConformancePlan used to surface
// its invalid CompareType/CollectionTopology only if execution
// happened to exercise it -- Compare/ElementKey's own PR #31/#32 panics
// are real, but ClassifyFieldValue never calls Compare at all unless
// coverage is Observed, and elementKeys never calls ElementKey at all
// if the collection's Path never resolves to any elements. Both
// reproduced directly before this fix: a plan built with either
// garbage value produced an ordinary ObjectVerdict, no panic at all.
// Fixed: ConformancePlan.Validate, called unconditionally at
// Evaluate's own entry, independent of which paths the rest of the
// walk happens to take for this particular intent/observed pair.
func TestMalformedPlanFailsClosedRegardlessOfExecutionPath(t *testing.T) {
	// The review's own first example: invalid CompareType on a field
	// whose coverage is Unobserved, so ClassifyFieldValue would never
	// reach Compare at all if Evaluate didn't validate the plan itself
	// up front.
	expectPanic(t, "invalid CompareType, field never Observed", func() {
		p := ConformancePlan{
			Scalars: []FieldPlan{
				{Path: "/x", Compare: conformance.CompareType("garbage")},
			},
		}
		Evaluate(
			map[string]interface{}{"x": 1},
			map[string]interface{}{"x": 1},
			conformance.NewObservation(), // "/x" defaults to Unobserved
			p,
		)
	})

	// The review's own second example: invalid CollectionTopology on a
	// collection whose Path never resolves to any elements, so
	// elementKeys would never call ElementKey at all if Evaluate didn't
	// validate the plan itself up front.
	expectPanic(t, "invalid CollectionTopology, collection path resolves to nothing", func() {
		p := ConformancePlan{
			Collections: []CollectionPlan{
				{
					Path:       "/missing",
					Collection: collection.Collection{Topology: collection.CollectionTopology("garbage")},
					Elements:   []FieldPlan{{Path: "y", Compare: conformance.CompareExact}},
				},
			},
		}
		Evaluate(
			map[string]interface{}{},
			map[string]interface{}{},
			conformance.NewObservation(),
			p,
		)
	})
}

func TestConformancePlanValidateAcceptsAWellFormedPlan(t *testing.T) {
	ociPlan().Validate() // must not panic
}

// fromCoverage is classifyAt's own internal constructor (see
// ConsumedField's doc comment) -- Coverage must still be one of the
// four defined values, the same boundary check conformance's own
// ClassifyField/Compare/Observation.Set already apply.
func TestFromCoverageFailsClosedOnInvalidCoverage(t *testing.T) {
	expectPanic(t, "fromCoverage", func() {
		fromCoverage(conformance.Coverage("garbage"), "x")
	})
}

func TestConsumedFieldOnlyCarriesAValueWhenObserved(t *testing.T) {
	observed := NewObservedConsumedField("x")
	if v, ok := observed.Value(); !ok || v != "x" {
		t.Errorf("observed: got (%v, %v), want (\"x\", true)", v, ok)
	}

	absent := NewAbsentConsumedField()
	unobserved := NewUnobservedConsumedField()
	unknown := NewUnknownConsumedField()
	for _, c := range []ConsumedField{absent, unobserved, unknown} {
		if v, ok := c.Value(); ok || v != nil {
			t.Errorf("%v: got (%v, %v), want (nil, false)", c.Coverage(), v, ok)
		}
	}
	if absent.Coverage() != conformance.CoverageAbsent {
		t.Errorf("Coverage() = %v, want %v", absent.Coverage(), conformance.CoverageAbsent)
	}
	if unobserved.Coverage() != conformance.CoverageUnobserved {
		t.Errorf("Coverage() = %v, want %v", unobserved.Coverage(), conformance.CoverageUnobserved)
	}
	if unknown.Coverage() != conformance.CoverageUnknown {
		t.Errorf("Coverage() = %v, want %v", unknown.Coverage(), conformance.CoverageUnknown)
	}
}

// Review-caught on PR #33: the PREVIOUS two-argument NewConsumedField
// silently accepted and discarded a value for any non-Observed
// coverage, rather than rejecting the call shape entirely --
// NewConsumedField(CoverageAbsent, "I should not exist") built a
// valid-looking ConsumedField with no error. Splitting into per-state
// constructors removes the parameter from the three states that must
// never carry one: there is no longer any way to even ATTEMPT passing
// a value into NewAbsentConsumedField/NewUnobservedConsumedField/
// NewUnknownConsumedField -- not "it's accepted and ignored," but
// "the argument position does not exist." This test exists to pin
// that shape, not to re-prove runtime behavior a type signature
// already guarantees at compile time.
func TestNonObservedConstructorsHaveNoValueParameter(t *testing.T) {
	// NewAbsentConsumedField() / NewUnobservedConsumedField() /
	// NewUnknownConsumedField() each take zero arguments -- this is
	// enforced by the compiler, not by this test; its only job is to
	// exist as a readable record of that fact. If a future change ever
	// added a value parameter back to any of these three, it would be
	// a regression back to the exact shape this PR's review rejected.
	_ = NewAbsentConsumedField()
	_ = NewUnobservedConsumedField()
	_ = NewUnknownConsumedField()
}

// Review-caught on PR #34: per-state constructors close off
// constructing a WRONG coverage/value combination, but Go's zero
// value exists for every struct regardless of whether any constructor
// was ever called -- var zero ConsumedField compiles to {coverage:
// "", value: nil} with no error. Valid() exists specifically to catch
// this at whatever boundary consumes a ConsumedField (go/
// digestprojection's own ObservationDigestProjection, in this
// package's case).
func TestZeroValueConsumedFieldIsNotValid(t *testing.T) {
	var zero ConsumedField
	if zero.Valid() {
		t.Error("the zero value must not be Valid()")
	}
	if v, ok := zero.Value(); ok || v != nil {
		t.Errorf("zero.Value() = (%v, %v), want (nil, false)", v, ok)
	}
}

func TestValidConsumedFieldsReportValid(t *testing.T) {
	for _, cf := range []ConsumedField{
		NewObservedConsumedField("x"),
		NewObservedConsumedField(nil), // observed, but the value itself is nil -- still valid
		NewAbsentConsumedField(),
		NewUnobservedConsumedField(),
		NewUnknownConsumedField(),
	} {
		if !cf.Valid() {
			t.Errorf("%+v should be Valid()", cf)
		}
	}
}
