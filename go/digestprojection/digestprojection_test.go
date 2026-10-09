package digestprojection

import (
	"reflect"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

// objectFromValue wraps a test fixture map into a
// materialized.MaterializedObject -- see go/plan's own identical
// helper for why this is test-only, not a production conversion.
func objectFromValue(
	t *testing.T,
	v map[string]interface{},
	evidenceOf func(key string, val interface{}) materialized.FieldEvidence,
) materialized.MaterializedObject {
	t.Helper()
	fields := make(map[string]materialized.MaterializedField, len(v))
	keys := make(map[string]struct{}, len(v))
	for k, val := range v {
		keys[k] = struct{}{}
		fields[k] = materialized.MaterializedField{
			Capability: materialized.CapabilityImplemented,
			Evidence:   evidenceOf(k, val),
		}
	}
	obj, err := materialized.NewMaterializedObject(fields, keys)
	if err != nil {
		t.Fatalf("a fixture's own key set must be trivially complete against itself: %v", err)
	}
	return obj
}

func intentObject(t *testing.T, v map[string]interface{}) materialized.MaterializedObject {
	t.Helper()
	return objectFromValue(t, v, func(_ string, val interface{}) materialized.FieldEvidence {
		return materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(val, true))
	})
}

func observedObject(t *testing.T, v map[string]interface{}, obs *conformance.Observation) materialized.MaterializedObject {
	t.Helper()
	return objectFromValue(t, v, func(k string, val interface{}) materialized.FieldEvidence {
		return materialized.NewObservationFieldEvidence(obs.Coverage("/"+k), val)
	})
}

// Mirrors engine/src/digest_projection.rs's
// plan_projection_has_f5s_exact_shape -- F5 names an exact shape, not
// just "scalar field paths and comparators", so this pins the actual
// field names/types down, rather than only trusting the doc prose.
func TestPlanProjectionHasF5sExactShape(t *testing.T) {
	cp := plan.ConformancePlan{
		Scalars: []plan.FieldPlan{
			{Path: "/b", Compare: conformance.CompareNumeric},
			{Path: "/a", Compare: conformance.CompareExact},
		},
		Collections: []plan.CollectionPlan{
			{
				Path: "/nics",
				Collection: collection.Collection{
					Topology: collection.TopologyMap,
					Keys:     []string{"zone", "name"},
				},
				Elements: []plan.FieldPlan{
					{Path: "subnet", Compare: conformance.CompareExact},
				},
			},
		},
	}
	got := PlanDigestProjection(cp)
	want := map[string]interface{}{
		"scalars": []interface{}{
			map[string]interface{}{"path": "/a", "compare": "exact"},
			map[string]interface{}{"path": "/b", "compare": "numeric"},
		},
		"collections": []interface{}{
			map[string]interface{}{
				"path":     "/nics",
				"topology": "map",
				"keys":     []interface{}{"name", "zone"},
				"elements": []interface{}{
					map[string]interface{}{"path": "subnet", "compare": "exact"},
				},
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}

// Mirrors digest_projection.rs's
// plan_digest_is_independent_of_declaration_order -- two plans that
// only differ in scalar/collection DECLARATION order must digest
// identically, the whole point of F5's own ordering rule.
func TestPlanDigestIsIndependentOfDeclarationOrder(t *testing.T) {
	a := plan.ConformancePlan{
		Scalars: []plan.FieldPlan{
			{Path: "/a", Compare: conformance.CompareExact},
			{Path: "/b", Compare: conformance.CompareExact},
		},
	}
	b := plan.ConformancePlan{
		Scalars: []plan.FieldPlan{
			{Path: "/b", Compare: conformance.CompareExact},
			{Path: "/a", Compare: conformance.CompareExact},
		},
	}
	da, err := ConformancePlanDigest(a)
	if err != nil {
		t.Fatal(err)
	}
	db, err := ConformancePlanDigest(b)
	if err != nil {
		t.Fatal(err)
	}
	if da != db {
		t.Errorf("digests differ by declaration order: %q vs %q", da, db)
	}
}

// Mirrors digest_projection.rs's
// observation_projection_only_carries_a_value_when_observed -- value
// is present iff coverage == observed, checked against all four B3
// coverage values via go/plan's own closed ConsumedField constructors
// (F13): there is no way to build the invalid case this test would
// otherwise also have to rule out.
func TestObservationProjectionOnlyCarriesAValueWhenObserved(t *testing.T) {
	consumed := map[string]plan.ConsumedField{
		"/observed":   plan.NewObservedConsumedField("v"),
		"/absent":     plan.NewAbsentConsumedField(),
		"/unobserved": plan.NewUnobservedConsumedField(),
		"/unknown":    plan.NewUnknownConsumedField(),
	}
	got := ObservationDigestProjection(consumed)
	want := map[string]interface{}{
		"fields": []interface{}{
			map[string]interface{}{"path": "/absent", "coverage": "absent"},
			map[string]interface{}{"path": "/observed", "coverage": "observed", "value": "v"},
			map[string]interface{}{"path": "/unknown", "coverage": "unknown"},
			map[string]interface{}{"path": "/unobserved", "coverage": "unobserved"},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("got %#v, want %#v", got, want)
	}
}

// Mirrors digest_projection.rs's
// observation_digest_distinguishes_different_observed_values --
// end-to-end: Evaluate's own Consumed output, fed straight into
// ObservationDigest, actually distinguishes a run where the observed
// VALUE differs from one where only coverage is identical -- the
// exact gap F5 widened Relay's envelope-only digest to close.
func TestObservationDigestDistinguishesDifferentObservedValues(t *testing.T) {
	cp := plan.ConformancePlan{
		Scalars: []plan.FieldPlan{{Path: "/shape", Compare: conformance.CompareExact}},
	}
	intent := map[string]interface{}{"shape": "E4.Flex"}
	obs := conformance.NewObservation()
	obs.Set("/shape", conformance.CoverageObserved)

	observedA := map[string]interface{}{"shape": "E4.Flex"}
	observedB := map[string]interface{}{"shape": "E3.Flex"}

	intentObj := intentObject(t, intent)
	verdictA := plan.Evaluate(intentObj, observedObject(t, observedA, obs), obs, cp)
	verdictB := plan.Evaluate(intentObj, observedObject(t, observedB, obs), obs, cp)

	digestA, err := ObservationDigest(verdictA.Consumed)
	if err != nil {
		t.Fatal(err)
	}
	digestB, err := ObservationDigest(verdictB.Consumed)
	if err != nil {
		t.Fatal(err)
	}
	// Same coverage on both sides -- Relay's own envelope-only digest
	// would have produced identical bytes for these two runs.
	if digestA == digestB {
		t.Error("observation digest must differ when the observed VALUE differs, even though coverage is identical on both sides")
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

// Applying the F13 review lesson here too: PlanDigestProjection is a
// second, independent entry point for a ConformancePlan (not only
// reachable through plan.Evaluate), so it must validate the plan on
// its own rather than trusting a caller who may never have called
// Evaluate at all -- without this, an invalid CompareType string
// would flow straight into the "compare" field (a bare string(...)
// conversion never rejects anything a Go string could hold).
func TestPlanDigestProjectionFailsClosedOnMalformedPlan(t *testing.T) {
	expectPanic(t, "invalid CompareType", func() {
		PlanDigestProjection(plan.ConformancePlan{
			Scalars: []plan.FieldPlan{{Path: "/x", Compare: conformance.CompareType("garbage")}},
		})
	})
	expectPanic(t, "invalid CollectionTopology", func() {
		PlanDigestProjection(plan.ConformancePlan{
			Collections: []plan.CollectionPlan{{
				Path:       "/c",
				Collection: collection.Collection{Topology: collection.CollectionTopology("garbage")},
			}},
		})
	})
}

// The review's own exact regression case: closing off ConsumedField's
// public constructors (PR #33) does not close off Go's zero value,
// which exists for every struct regardless of whether any constructor
// was ever called. var zero plan.ConsumedField is {coverage: "",
// value: nil} -- not one of the four legitimate states -- and before
// this fix it flowed straight through to a legitimate-looking
// {"path": "/x", "coverage": ""} entry, canonicalized and SHA-256'd
// with no error at all.
func TestObservationProjectionRejectsZeroConsumedField(t *testing.T) {
	var zero plan.ConsumedField
	expectPanic(t, "zero-value ConsumedField", func() {
		ObservationDigestProjection(map[string]plan.ConsumedField{"/x": zero})
	})
}

// oneFieldObject wraps a single field into a MaterializedObject --
// mirrors digest_projection.rs's own one_field_object test helper.
func oneFieldObject(t *testing.T, field materialized.MaterializedField) materialized.MaterializedObject {
	t.Helper()
	fields := map[string]materialized.MaterializedField{"x": field}
	obj, err := materialized.NewMaterializedObject(fields, map[string]struct{}{"x": {}})
	if err != nil {
		t.Fatalf("single key, trivially complete: %v", err)
	}
	return obj
}

// Mirrors digest_projection.rs's
// materialized_object_projection_has_the_decided_shape_per_evidence_kind
// -- pins the exact per-kind shape the package doc comment decides,
// not just trusting the doc prose, the same discipline
// TestPlanProjectionHasF5sExactShape already applies to PlanDigestProjection.
func TestMaterializedObjectProjectionHasTheDecidedShapePerEvidenceKind(t *testing.T) {
	intentField := materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence("E4.Flex", true)),
	}
	got := MaterializedObjectProjection(oneFieldObject(t, intentField))
	want := map[string]interface{}{
		"fields": []interface{}{
			map[string]interface{}{
				"key":        "x",
				"capability": "implemented",
				"provenance": "authored",
				"value":      "E4.Flex",
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("intent field: got %#v, want %#v", got, want)
	}

	observationField := materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewObservationFieldEvidence(conformance.CoverageObserved, 16),
	}
	got = MaterializedObjectProjection(oneFieldObject(t, observationField))
	want = map[string]interface{}{
		"fields": []interface{}{
			map[string]interface{}{
				"key":        "x",
				"capability": "implemented",
				"coverage":   "observed",
				"value":      16,
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("observation field: got %#v, want %#v", got, want)
	}

	// B3: "a derived state field carries both... simultaneously" --
	// coverage AND provenance, provenance always exactly "derived".
	derivedField := materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewDerivedObservationFieldEvidence(conformance.CoverageObserved, "RUNNING"),
	}
	got = MaterializedObjectProjection(oneFieldObject(t, derivedField))
	want = map[string]interface{}{
		"fields": []interface{}{
			map[string]interface{}{
				"key":        "x",
				"capability": "implemented",
				"coverage":   "observed",
				"provenance": "derived",
				"value":      "RUNNING",
			},
		},
	}
	if !reflect.DeepEqual(got, want) {
		t.Errorf("derived observation field: got %#v, want %#v", got, want)
	}
}

// Mirrors digest_projection.rs's
// materialized_object_projection_omits_value_rather_than_padding_with_null
// -- BOUNDARY.md's own anti-placeholder principle: no value present
// means no "value" key at all, never a null placeholder.
func TestMaterializedObjectProjectionOmitsValueRatherThanPaddingWithNull(t *testing.T) {
	absentField := materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewObservationFieldEvidence(conformance.CoverageAbsent, nil),
	}
	got := MaterializedObjectProjection(oneFieldObject(t, absentField))
	entry := got["fields"].([]interface{})[0].(map[string]interface{})
	if _, ok := entry["value"]; ok {
		t.Errorf("an Absent field must have no \"value\" key at all, got %#v", entry)
	}

	// B2's still-open authored-absent case.
	authoredAbsentField := materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(nil, false)),
	}
	got = MaterializedObjectProjection(oneFieldObject(t, authoredAbsentField))
	entry = got["fields"].([]interface{})[0].(map[string]interface{})
	if _, ok := entry["value"]; ok {
		t.Errorf("an authored-absent field must have no \"value\" key at all, got %#v", entry)
	}
}

// Mirrors digest_projection.rs's
// materialized_object_digest_distinguishes_different_values.
func TestMaterializedObjectDigestDistinguishesDifferentValues(t *testing.T) {
	a := oneFieldObject(t, materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewObservationFieldEvidence(conformance.CoverageObserved, 16),
	})
	b := oneFieldObject(t, materialized.MaterializedField{
		Capability: materialized.CapabilityImplemented,
		Evidence:   materialized.NewObservationFieldEvidence(conformance.CoverageObserved, 32),
	})
	da, err := MaterializedObjectDigest(a)
	if err != nil {
		t.Fatal(err)
	}
	db, err := MaterializedObjectDigest(b)
	if err != nil {
		t.Fatal(err)
	}
	if da == db {
		t.Error("materialized object digest must differ when the value differs")
	}
}
