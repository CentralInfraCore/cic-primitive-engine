package verdict

import (
	"errors"
	"strings"
	"testing"

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
