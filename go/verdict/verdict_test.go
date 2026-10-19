package verdict

import (
	"errors"
	"strings"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/digestprojection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

// sample mirrors engine/src/verdict.rs's own test fixture: two planned
// scalars, one matching and one drifting.
func sample(t *testing.T) (plan.ConformancePlan, plan.ObjectVerdict) {
	t.Helper()
	cp := plan.ConformancePlan{Scalars: []plan.FieldPlan{
		{Path: "/shape", Compare: conformance.CompareExact},
		{Path: "/memory_gb", Compare: conformance.CompareNumeric},
	}}
	obs := conformance.NewObservation()
	obs.Set("/shape", conformance.CoverageObserved)
	obs.Set("/memory_gb", conformance.CoverageObserved)
	intent := objectFromValue(t, map[string]interface{}{"shape": "E4", "memory_gb": 16},
		func(_ string, val interface{}) materialized.FieldEvidence {
			return materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(val, true))
		})
	observed := objectFromValue(t, map[string]interface{}{"shape": "E4", "memory_gb": 32.0},
		func(k string, val interface{}) materialized.FieldEvidence {
			return materialized.NewObservationFieldEvidence(obs.Coverage("/"+k), val)
		})
	return cp, plan.Evaluate(intent, observed, obs, cp)
}

func wantInconsistent(t *testing.T, err error, path string) {
	t.Helper()
	var verr *Error
	if !errors.As(err, &verr) {
		t.Fatalf("want *Error, got %v", err)
	}
	if verr.Code != CodeInconsistentVerdict || verr.Path != path {
		t.Fatalf("got %s at %s, want %s at %s", verr.Code, verr.Path, CodeInconsistentVerdict, path)
	}
}

func TestARealEvaluateResultBuildsAllFiveFields(t *testing.T) {
	cp, result := sample(t)
	v, err := Build(cp, result)
	if err != nil {
		t.Fatal(err)
	}
	if v.SchemaVersion != 1 || v.Object != plan.ObjectDrift || len(v.Fields) != 2 {
		t.Fatalf("unexpected artifact: %+v", v)
	}
	if !strings.HasPrefix(v.ConformancePlanDigest, "sha256:") || !strings.HasPrefix(v.ObservationDigest, "sha256:") {
		t.Fatalf("digests are not sha256: %q %q", v.ConformancePlanDigest, v.ObservationDigest)
	}
}

func TestTheDigestsAreTheExistingProjectionsNotANewComputation(t *testing.T) {
	cp, result := sample(t)
	v, err := Build(cp, result)
	if err != nil {
		t.Fatal(err)
	}
	wantPlan, _ := digestprojection.ConformancePlanDigest(cp)
	wantObs, _ := digestprojection.ObservationDigest(result.Consumed)
	if v.ConformancePlanDigest != wantPlan || v.ObservationDigest != wantObs {
		t.Fatal("verdict digests differ from the digestprojection functions")
	}
}

func TestFieldsHoldExactlyTheEvaluatedPathsAndDoNotAliasTheResult(t *testing.T) {
	cp, result := sample(t)
	v, err := Build(cp, result)
	if err != nil {
		t.Fatal(err)
	}
	if len(v.Fields) != len(result.Fields) {
		t.Fatalf("fields %d, evaluated %d", len(v.Fields), len(result.Fields))
	}
	result.Fields["/shape"] = conformance.VerdictDrift
	if v.Fields["/shape"] != conformance.VerdictConformant {
		t.Fatal("the artifact aliases the Evaluate result's map")
	}
}

func TestTheValueTreeHasExactlyTheFiveRequiredKeys(t *testing.T) {
	cp, result := sample(t)
	v, _ := Build(cp, result)
	tree := v.Value()
	want := []string{"conformance_plan_digest", "fields", "object", "observation_digest", "verdict_schema_version"}
	if len(tree) != len(want) {
		t.Fatalf("got %d keys, want %d", len(tree), len(want))
	}
	for _, k := range want {
		if _, ok := tree[k]; !ok {
			t.Fatalf("missing key %s", k)
		}
	}
	if tree["verdict_schema_version"] != int64(1) {
		t.Fatalf("version is %v", tree["verdict_schema_version"])
	}
}

func TestCanonicalBytesAreSortedAndStable(t *testing.T) {
	cp, result := sample(t)
	v, _ := Build(cp, result)
	a, err := v.CanonicalBytes()
	if err != nil {
		t.Fatal(err)
	}
	b, _ := v.CanonicalBytes()
	if string(a) != string(b) {
		t.Fatal("canonical bytes are not stable")
	}
	s := string(a)
	if !strings.HasPrefix(s, `{"conformance_plan_digest":"sha256:`) || !strings.HasSuffix(s, `"verdict_schema_version":1}`) {
		t.Fatalf("unexpected canonical form: %s", s)
	}
}

func TestAClassifiedPathWithoutEvidenceIsRefused(t *testing.T) {
	cp, result := sample(t)
	delete(result.Consumed, "/shape")
	_, err := Build(cp, result)
	wantInconsistent(t, err, "/shape")
}

func TestEvidenceForAnUnclassifiedPathIsRefused(t *testing.T) {
	cp, result := sample(t)
	result.Consumed["/ghost"] = plan.NewAbsentConsumedField()
	_, err := Build(cp, result)
	wantInconsistent(t, err, "/ghost")
}

func TestAPlannedScalarMissingFromFieldsIsRefused(t *testing.T) {
	cp, result := sample(t)
	delete(result.Fields, "/memory_gb")
	delete(result.Consumed, "/memory_gb")
	_, err := Build(cp, result)
	wantInconsistent(t, err, "/memory_gb")
}

func TestAnObjectThatDoesNotFollowFromFieldsIsRefused(t *testing.T) {
	cp, result := sample(t)
	result.Object = plan.ObjectConformant
	_, err := Build(cp, result)
	wantInconsistent(t, err, "$")
}

// Go-only: the zero ConsumedField is not one of the four states a
// constructor produces, and Rust cannot construct the equivalent at all.
func TestAZeroValueConsumedFieldIsRefusedNotDigested(t *testing.T) {
	cp, result := sample(t)
	result.Consumed["/shape"] = plan.ConsumedField{}
	_, err := Build(cp, result)
	wantInconsistent(t, err, "/shape")
}

// Counterexample 1 (review of PR #60): /ghost is in both Fields and Consumed,
// so the two path sets agree and Object follows, but the plan never asked
// for it.
func TestCounterexampleAPathOutsideThePlanIsRefused(t *testing.T) {
	cp, result := sample(t)
	result.Fields["/ghost"] = conformance.VerdictConformant
	result.Consumed["/ghost"] = plan.NewAbsentConsumedField()
	_, err := Build(cp, result)
	wantInconsistent(t, err, "/ghost")
}

func tieInputs(t *testing.T) (materialized.MaterializedObject, materialized.MaterializedObject, *conformance.Observation) {
	t.Helper()
	obs := conformance.NewObservation()
	obs.Set("/memory_gb", conformance.CoverageObserved)
	intent := objectFromValue(t, map[string]interface{}{"memory_gb": 16},
		func(_ string, val interface{}) materialized.FieldEvidence {
			return materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(val, true))
		})
	observed := objectFromValue(t, map[string]interface{}{"memory_gb": "16"},
		func(k string, val interface{}) materialized.FieldEvidence {
			return materialized.NewObservationFieldEvidence(obs.Coverage("/"+k), val)
		})
	return intent, observed, obs
}

func memoryPlan(c conformance.CompareType) plan.ConformancePlan {
	return plan.ConformancePlan{Scalars: []plan.FieldPlan{{Path: "/memory_gb", Compare: c}}}
}

// Counterexample 2 (review of PR #60): the result was produced under planA
// but Build is handed planB with the same paths and a different comparator.
// The low-level builder cannot tell: it accepts and writes planB's digest
// next to planA's verdicts. This is the documented limit of Build, pinned
// here so it cannot be mistaken for a guarantee.
func TestCounterexampleTheLowLevelBuilderDoesNotBindThePlan(t *testing.T) {
	intent, observed, obs := tieInputs(t)
	planA := memoryPlan(conformance.CompareNumeric)
	planB := memoryPlan(conformance.CompareExact)
	resultA := plan.Evaluate(intent, observed, obs, planA)

	mixed, err := Build(planB, resultA)
	if err != nil {
		t.Fatalf("structure alone looks fine: %v", err)
	}
	digestA, _ := digestprojection.ConformancePlanDigest(planA)
	digestB, _ := digestprojection.ConformancePlanDigest(planB)
	if mixed.ConformancePlanDigest != digestB {
		t.Fatal("the artifact should carry the digest of the plan it was handed")
	}
	if mixed.ConformancePlanDigest == digestA {
		t.Fatal("the artifact names the plan that produced its verdicts: not the counterexample")
	}
}

// The answer to counterexample 2: through the primary entry point the plan
// and the result cannot be paired independently, so each plan's digest sits
// next to the verdicts that plan actually produced.
func TestCounterexampleTheEntryPointBindsThePlanToItsResult(t *testing.T) {
	intent, observed, obs := tieInputs(t)
	planA := memoryPlan(conformance.CompareNumeric)
	planB := memoryPlan(conformance.CompareExact)
	a, err := EvaluateAndBuild(intent, observed, obs, planA)
	if err != nil {
		t.Fatal(err)
	}
	b, err := EvaluateAndBuild(intent, observed, obs, planB)
	if err != nil {
		t.Fatal(err)
	}
	digestA, _ := digestprojection.ConformancePlanDigest(planA)
	digestB, _ := digestprojection.ConformancePlanDigest(planB)
	if a.ConformancePlanDigest != digestA || b.ConformancePlanDigest != digestB || digestA == digestB {
		t.Fatal("each artifact must carry the digest of the plan that produced it")
	}
	// The comparator really changes the verdict, so the digest is not decoration.
	if a.Fields["/memory_gb"] == b.Fields["/memory_gb"] {
		t.Fatalf("the two plans should classify differently, both gave %s", a.Fields["/memory_gb"])
	}
	// Each artifact is exactly what the low-level builder gives for the
	// result of the SAME plan.
	direct, err := Build(planB, plan.Evaluate(intent, observed, obs, planB))
	if err != nil {
		t.Fatal(err)
	}
	gotBytes, _ := b.CanonicalBytes()
	wantBytes, _ := direct.CanonicalBytes()
	if string(gotBytes) != string(wantBytes) {
		t.Fatal("EvaluateAndBuild differs from Build over the same plan's result")
	}
}

func TestCollectionElementPathsAreMatchedByShapeAndForeignOnesRefused(t *testing.T) {
	cp := plan.ConformancePlan{Collections: []plan.CollectionPlan{{
		Path: "/nics",
		Collection: collection.Collection{
			Topology: collection.TopologyMap,
			Keys:     []string{"name"},
		},
		Elements: []plan.FieldPlan{{Path: "subnet", Compare: conformance.CompareExact}},
	}}}
	with := func(path string) plan.ObjectVerdict {
		return plan.ObjectVerdict{
			Object:   plan.ObjectConformant,
			Fields:   map[string]conformance.FieldVerdict{path: conformance.VerdictConformant},
			Consumed: map[string]plan.ConsumedField{path: plan.NewAbsentConsumedField()},
		}
	}
	if _, err := Build(cp, with("/nics/{name=a}/subnet")); err != nil {
		t.Fatalf("a path of the planned shape is accepted: %v", err)
	}
	for _, foreign := range []string{"/nics/{name=a}/other", "/other/{name=a}/subnet", "/nics/{}/subnet", "/nics/subnet"} {
		_, err := Build(cp, with(foreign))
		wantInconsistent(t, err, foreign)
	}
}
