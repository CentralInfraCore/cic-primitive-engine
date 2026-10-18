package verdict

// Runs the verdict group of the language-independent differential corpus in
// conformance/differential/ -- the Go peer of engine/tests/differential.rs's
// verdict_vectors and verdict_vectors_detect_deliberate_divergence, reading
// the identical JSON files. See that directory's README for why the corpus
// is JSON-only.

import (
	"bytes"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"sort"
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
	"github.com/CentralInfraCore/cic-primitive-engine/go/materialized"
	"github.com/CentralInfraCore/cic-primitive-engine/go/plan"
)

func corpusRoot(group string) string {
	// go test runs in this package's directory (go/verdict/).
	return filepath.Join("..", "..", "conformance", "differential", group)
}

type fieldPlanInput struct {
	Path    string `json:"path"`
	Compare string `json:"compare"`
}

type tamperOp struct {
	Op    string `json:"op"`
	Path  string `json:"path"`
	Value string `json:"value"`
}

type vectorInput struct {
	Intent      map[string]interface{} `json:"intent"`
	Observed    map[string]interface{} `json:"observed"`
	Observation []struct {
		Path     string `json:"path"`
		Coverage string `json:"coverage"`
	} `json:"observation"`
	Plan struct {
		Scalars     []fieldPlanInput `json:"scalars"`
		Collections []struct {
			Path     string           `json:"path"`
			Topology string           `json:"topology"`
			Keys     []string         `json:"keys"`
			Elements []fieldPlanInput `json:"elements"`
		} `json:"collections"`
	} `json:"plan"`
	Tamper []tamperOp `json:"tamper"`
}

type vectorExpected struct {
	Verdict   json.RawMessage `json:"verdict"`
	Canonical string          `json:"canonical"`
	Error     *struct {
		Code string `json:"code"`
		Path string `json:"path"`
	} `json:"error"`
}

func readJSON(t *testing.T, path string, v interface{}) {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("%s: %v", path, err)
	}
	if err := json.Unmarshal(b, v); err != nil {
		t.Fatalf("%s: %v", path, err)
	}
}

func vectorNames(t *testing.T, group string) []string {
	t.Helper()
	entries, err := os.ReadDir(corpusRoot(group))
	if err != nil {
		t.Fatalf("%s: %v", group, err)
	}
	var names []string
	for _, e := range entries {
		if e.IsDir() {
			names = append(names, e.Name())
		}
	}
	sort.Strings(names)
	if len(names) == 0 {
		t.Fatalf("%s: no vectors -- an empty corpus trivially passes", group)
	}
	return names
}

// objectFromValue wraps a fixture map into a MaterializedObject, the same
// test-only convenience go/plan's own tests use.
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
		t.Fatalf("a fixture's own key set is trivially complete against itself: %v", err)
	}
	return obj
}

func vectorObjects(t *testing.T, in vectorInput) (plan.ConformancePlan, materialized.MaterializedObject, materialized.MaterializedObject, *conformance.Observation) {
	t.Helper()
	obs := conformance.NewObservation()
	for _, o := range in.Observation {
		obs.Set(o.Path, conformance.Coverage(o.Coverage))
	}
	toPlan := func(fp fieldPlanInput) plan.FieldPlan {
		return plan.FieldPlan{Path: fp.Path, Compare: conformance.CompareType(fp.Compare)}
	}
	scalars := make([]plan.FieldPlan, len(in.Plan.Scalars))
	for i, fp := range in.Plan.Scalars {
		scalars[i] = toPlan(fp)
	}
	collections := make([]plan.CollectionPlan, len(in.Plan.Collections))
	for i, cp := range in.Plan.Collections {
		elements := make([]plan.FieldPlan, len(cp.Elements))
		for j, fp := range cp.Elements {
			elements[j] = toPlan(fp)
		}
		collections[i] = plan.CollectionPlan{
			Path: cp.Path,
			Collection: collection.Collection{
				Topology: collection.CollectionTopology(cp.Topology),
				Keys:     cp.Keys,
			},
			Elements: elements,
		}
	}
	cp := plan.ConformancePlan{Scalars: scalars, Collections: collections}

	intent := objectFromValue(t, in.Intent, func(_ string, val interface{}) materialized.FieldEvidence {
		return materialized.NewIntentFieldEvidence(materialized.NewAuthoredIntentEvidence(val, true))
	})
	observed := objectFromValue(t, in.Observed, func(k string, val interface{}) materialized.FieldEvidence {
		if _, isContainer := val.([]interface{}); isContainer {
			return materialized.NewObservationFieldEvidence(conformance.CoverageObserved, val)
		}
		return materialized.NewObservationFieldEvidence(obs.Coverage("/"+k), val)
	})
	return cp, intent, observed, obs
}

func evaluateVector(t *testing.T, in vectorInput) (plan.ConformancePlan, plan.ObjectVerdict) {
	t.Helper()
	cp, intent, observed, obs := vectorObjects(t, in)
	result := plan.Evaluate(intent, observed, obs, cp)
	applyTamper(t, &result, in.Tamper)
	return cp, result
}

// applyTamper edits the Evaluate result before the artifact is built, to
// reach states a real Evaluate never produces.
func applyTamper(t *testing.T, result *plan.ObjectVerdict, ops []tamperOp) {
	t.Helper()
	for _, op := range ops {
		switch op.Op {
		case "drop_consumed":
			delete(result.Consumed, op.Path)
		case "add_consumed":
			result.Consumed[op.Path] = plan.NewAbsentConsumedField()
		case "drop_field":
			delete(result.Fields, op.Path)
		case "set_object":
			result.Object = plan.ObjectConformance(op.Value)
		default:
			t.Fatalf("unknown tamper op %q", op.Op)
		}
	}
}

// mismatches is empty when the artifact matches the pinned expectation
// exactly: the canonical bytes equal the pinned string, and the pinned
// verdict tree canonicalizes to those same bytes.
func mismatches(t *testing.T, got Verdict, exp vectorExpected) []string {
	t.Helper()
	var out []string
	b, err := got.CanonicalBytes()
	if err != nil {
		t.Fatalf("a verdict canonicalizes: %v", err)
	}
	if string(b) != exp.Canonical {
		out = append(out, "canonical bytes differ\n    got:  "+string(b)+"\n    want: "+exp.Canonical)
	}
	dec := json.NewDecoder(bytes.NewReader(exp.Verdict))
	dec.UseNumber()
	var tree interface{}
	if err := dec.Decode(&tree); err != nil {
		t.Fatalf("pinned verdict tree: %v", err)
	}
	treeBytes, err := canonical.ToCanonicalJSON(tree)
	if err != nil {
		t.Fatalf("pinned verdict tree canonicalizes: %v", err)
	}
	if !bytes.Equal(treeBytes, b) {
		out = append(out, "pinned verdict tree and pinned canonical string disagree with the artifact")
	}
	return out
}

type planExpected struct {
	Object string            `json:"object"`
	Fields map[string]string `json:"fields"`
}

func TestDifferentialVerdictVectors(t *testing.T) {
	names := vectorNames(t, "verdict")
	planNames := map[string]bool{}
	for _, n := range vectorNames(t, "plan") {
		planNames[n] = true
	}
	successes, refusals := 0, 0
	for _, name := range names {
		dir := filepath.Join(corpusRoot("verdict"), name)
		var in vectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		var exp vectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)

		cp, result := evaluateVector(t, in)
		got, err := Build(cp, result)

		if exp.Error != nil {
			refusals++
			var verr *Error
			if err == nil {
				t.Errorf("%s: expected a refusal, got an artifact", name)
			} else if !errors.As(err, &verr) {
				t.Errorf("%s: refusal is not a *Error: %v", name, err)
			} else if verr.Code != exp.Error.Code || verr.Path != exp.Error.Path {
				t.Errorf("%s: refusal got %s at %s, want %s at %s", name, verr.Code, verr.Path, exp.Error.Code, exp.Error.Path)
			}
			continue
		}

		successes++
		if err != nil {
			t.Errorf("%s: unexpected refusal: %v", name, err)
			continue
		}
		for _, m := range mismatches(t, got, exp) {
			t.Errorf("%s: %s", name, m)
		}
		// The artifact's Object/Fields are the plan group's own pinned
		// expectation for the same input, not a second opinion.
		if planNames[name] {
			var pe planExpected
			readJSON(t, filepath.Join(corpusRoot("plan"), name, "expected.json"), &pe)
			gotFields := make(map[string]string, len(got.Fields))
			for k, v := range got.Fields {
				gotFields[k] = string(v)
			}
			if string(got.Object) != pe.Object || len(gotFields) != len(pe.Fields) {
				t.Errorf("%s: object/fields differ from the plan group's pin", name)
				continue
			}
			for k, want := range pe.Fields {
				if gotFields[k] != want {
					t.Errorf("%s: field %s: got %s, want %s", name, k, gotFields[k], want)
				}
			}
		}
	}
	if successes == 0 || refusals == 0 {
		t.Fatalf("the corpus needs both artifacts and refusals (got %d / %d)", successes, refusals)
	}
}

func flipHex(d string) string {
	b := []byte(d)
	last := len(b) - 1
	if b[last] == '0' {
		b[last] = '1'
	} else {
		b[last] = '0'
	}
	return string(b)
}

func firstField(v Verdict) string {
	keys := make([]string, 0, len(v.Fields))
	for k := range v.Fields {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys[0]
}

// The corpus must be able to fail: each deliberate divergence of an
// otherwise correct artifact has to be caught by the very comparison the
// vectors use. Same list as the Rust peer's verdict_vectors_detect_deliberate_divergence.
func TestDifferentialVerdictVectorsDetectDeliberateDivergence(t *testing.T) {
	mutations := []struct {
		what   string
		mutate func(v *Verdict)
	}{
		{"schema version bumped", func(v *Verdict) { v.SchemaVersion++ }},
		{"object changed", func(v *Verdict) {
			if v.Object == plan.ObjectConformant {
				v.Object = plan.ObjectDrift
			} else {
				v.Object = plan.ObjectConformant
			}
		}},
		{"one field verdict changed", func(v *Verdict) {
			k := firstField(*v)
			if v.Fields[k] == conformance.VerdictDrift {
				v.Fields[k] = conformance.VerdictConformant
			} else {
				v.Fields[k] = conformance.VerdictDrift
			}
		}},
		{"one field dropped", func(v *Verdict) { delete(v.Fields, firstField(*v)) }},
		{"one field added", func(v *Verdict) { v.Fields["/extra"] = conformance.VerdictConformant }},
		{"plan digest altered", func(v *Verdict) { v.ConformancePlanDigest = flipHex(v.ConformancePlanDigest) }},
		{"observation digest altered", func(v *Verdict) { v.ObservationDigest = flipHex(v.ObservationDigest) }},
		{"plan and observation digests swapped", func(v *Verdict) {
			v.ConformancePlanDigest, v.ObservationDigest = v.ObservationDigest, v.ConformancePlanDigest
		}},
	}

	checked := 0
	for _, name := range vectorNames(t, "verdict") {
		dir := filepath.Join(corpusRoot("verdict"), name)
		var exp vectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)
		if exp.Error != nil {
			continue
		}
		var in vectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		cp, result := evaluateVector(t, in)
		good, err := Build(cp, result)
		if err != nil {
			t.Fatalf("%s: a success vector builds: %v", name, err)
		}
		if m := mismatches(t, good, exp); len(m) != 0 {
			t.Fatalf("%s: the unmodified artifact must match first: %v", name, m)
		}
		for _, mu := range mutations {
			bad := good
			bad.Fields = make(map[string]conformance.FieldVerdict, len(good.Fields)+1)
			for k, v := range good.Fields {
				bad.Fields[k] = v
			}
			mu.mutate(&bad)
			if len(mismatches(t, bad, exp)) == 0 {
				t.Errorf("%s: divergence not detected: %s", name, mu.what)
			}
			checked++
		}
	}
	if checked == 0 {
		t.Fatal("no success vector was mutated")
	}
}

// The primary entry point must give the pinned bytes too: every artifact
// vector (the corpus is unchanged), evaluated and assembled in one
// operation, matches the same pin the low-level builder is held to.
func TestDifferentialVerdictVectorsThroughTheEntryPoint(t *testing.T) {
	checked := 0
	for _, name := range vectorNames(t, "verdict") {
		dir := filepath.Join(corpusRoot("verdict"), name)
		var exp vectorExpected
		readJSON(t, filepath.Join(dir, "expected.json"), &exp)
		if exp.Error != nil {
			continue
		}
		var in vectorInput
		readJSON(t, filepath.Join(dir, "input.json"), &in)
		in.Tamper = nil
		cp, intent, observed, obs := vectorObjects(t, in)
		got, err := EvaluateAndBuild(intent, observed, obs, cp)
		if err != nil {
			t.Errorf("%s: %v", name, err)
			continue
		}
		for _, m := range mismatches(t, got, exp) {
			t.Errorf("%s: %s", name, m)
		}
		checked++
	}
	if checked == 0 {
		t.Fatal("no artifact vector was checked")
	}
}
