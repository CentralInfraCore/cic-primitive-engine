package digestprojection

import (
	"reflect"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

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

	verdictA := plan.Evaluate(intent, observedA, obs, cp)
	verdictB := plan.Evaluate(intent, observedB, obs, cp)

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
