package materialized

import (
	"testing"

	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
)

func TestAxisStringsMatchTheSchemaAndBoundaryNames(t *testing.T) {
	cases := []struct {
		got  string
		want string
	}{
		{string(CapabilityImplemented), "implemented"},
		{string(CapabilityNotImplemented), "not_implemented"},
		{string(CapabilityDeprecated), "deprecated"},
		{string(ProvenanceAuthored), "authored"},
		{string(ProvenanceSchemaDefault), "schema_default"},
		{string(ProvenanceDerived), "derived"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("got %q, want %q", c.got, c.want)
		}
	}
}

// B3: "a field materialized from plain authored intent still carries a
// provenance and no coverage" -- Value() must read straight through
// IntentEvidence's own flag, no fallback invented.
func TestIntentValueReadsThroughDirectly(t *testing.T) {
	f := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewIntentFieldEvidence(NewAuthoredIntentEvidence("prod", true)),
	}
	v, ok := f.Value()
	if !ok || v != "prod" {
		t.Errorf("got (%v, %v), want (\"prod\", true)", v, ok)
	}
	if !f.Evidence.Valid() {
		t.Error("expected Valid() to be true")
	}
}

// B2's still-open authored-absent question: hasValue=false under
// ProvenanceAuthored must stay representable and readable as "no
// value," not panic or get coerced into something else.
func TestAuthoredAbsentHasNoValueAndDoesNotPanic(t *testing.T) {
	f := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewIntentFieldEvidence(NewAuthoredIntentEvidence(nil, false)),
	}
	if _, ok := f.Value(); ok {
		t.Error("expected no value")
	}
	if !f.Evidence.Valid() {
		t.Error("expected Valid() to be true -- authored-absent is a legitimate state")
	}
	ie := NewAuthoredIntentEvidence("ignored, must be discarded", false)
	if v, ok := ie.Value(); ok || v != nil {
		t.Errorf("got (%v, %v), want (nil, false) -- a non-nil value with hasValue=false must be normalized away", v, ok)
	}
}

// Review-caught on PR #40 (Rust side): SchemaDefault/Derived always
// carry a real value (B1: "substituted"/"computed" -- both name an
// operation that always produces one), unlike Authored. The Go peer
// makes a SchemaDefault/Derived + no-value combination impossible to
// construct at all -- there is no hasValue parameter for either
// constructor -- so there is nothing to assert a rejection of, which
// is the point.
func TestSchemaDefaultAndDerivedAlwaysCarryARealValue(t *testing.T) {
	defaulted := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewIntentFieldEvidence(NewSchemaDefaultIntentEvidence(0)),
	}
	v, ok := defaulted.Value()
	if !ok || v != 0 {
		t.Errorf("got (%v, %v), want (0, true)", v, ok)
	}
	if got := defaulted.Evidence.intent.Provenance(); got != ProvenanceSchemaDefault {
		t.Errorf("got provenance %q, want %q", got, ProvenanceSchemaDefault)
	}

	derived := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewIntentFieldEvidence(NewDerivedIntentEvidence(true)),
	}
	v, ok = derived.Value()
	if !ok || v != true {
		t.Errorf("got (%v, %v), want (true, true)", v, ok)
	}
	if got := derived.Evidence.intent.Provenance(); got != ProvenanceDerived {
		t.Errorf("got provenance %q, want %q", got, ProvenanceDerived)
	}
}

// B3: "a field materialized from a raw, non-derived observation still
// carries a coverage and no provenance" -- and the value/coverage
// pairing is enforced at construction (newObservationLikeFieldEvidence),
// not re-checked here.
func TestObservationValueComesFromCoverage(t *testing.T) {
	f := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewObservationFieldEvidence(conformance.CoverageObserved, 16),
	}
	v, ok := f.Value()
	if !ok || v != 16 {
		t.Errorf("got (%v, %v), want (16, true)", v, ok)
	}

	absent := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewObservationFieldEvidence(conformance.CoverageAbsent, "ignored, must be discarded"),
	}
	if _, ok := absent.Value(); ok {
		t.Error("expected no value for CoverageAbsent")
	}
}

// B3: "only the derived case populates both" -- DerivedObservation
// carries a real value when Observed, and Derived provenance is
// implied by the constructor itself, not a field that could disagree.
func TestDerivedObservationValueComesFromCoverageToo(t *testing.T) {
	f := MaterializedField{
		Capability: CapabilityImplemented,
		Evidence:   NewDerivedObservationFieldEvidence(conformance.CoverageObserved, "RUNNING"),
	}
	v, ok := f.Value()
	if !ok || v != "RUNNING" {
		t.Errorf("got (%v, %v), want (\"RUNNING\", true)", v, ok)
	}
}

func TestObservationFieldEvidencePanicsOnInvalidCoverage(t *testing.T) {
	defer func() {
		if recover() == nil {
			t.Error("expected a panic for an invalid Coverage")
		}
	}()
	NewObservationFieldEvidence(conformance.Coverage("garbage"), nil)
}

// Review-caught pattern this crate applies everywhere a Go type stands
// in for a Rust closed enum (F13/F14, this package's own
// IntentEvidence): the zero value exists for every struct regardless
// of constructor discipline, and must not silently look legitimate.
func TestZeroValueFieldEvidenceIsInvalid(t *testing.T) {
	var fe FieldEvidence
	if fe.Valid() {
		t.Error("expected the zero FieldEvidence to be invalid")
	}
	var ie IntentEvidence
	if ie.Valid() {
		t.Error("expected the zero IntentEvidence to be invalid")
	}
}

func keySet(keys ...string) map[string]struct{} {
	s := make(map[string]struct{}, len(keys))
	for _, k := range keys {
		s[k] = struct{}{}
	}
	return s
}

func TestNewMaterializedObjectAcceptsAnExactKeyMatch(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape":     {Capability: CapabilityImplemented, Evidence: NewIntentFieldEvidence(NewAuthoredIntentEvidence("E4.Flex", true))},
		"memory_gb": {Capability: CapabilityImplemented, Evidence: NewObservationFieldEvidence(conformance.CoverageObserved, 16)},
	}
	obj, err := NewMaterializedObject(fields, keySet("shape", "memory_gb"))
	if err != nil {
		t.Fatalf("expected an exact key match to be accepted, got %v", err)
	}
	if obj.Len() != 2 {
		t.Errorf("got Len() %d, want 2", obj.Len())
	}
	if _, ok := obj.Get("shape"); !ok {
		t.Error("expected \"shape\" to be present")
	}
}

func TestNewMaterializedObjectRejectsAMissingKey(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape": {Capability: CapabilityImplemented, Evidence: NewIntentFieldEvidence(NewAuthoredIntentEvidence("E4.Flex", true))},
	}
	if _, err := NewMaterializedObject(fields, keySet("shape", "memory_gb")); err == nil {
		t.Error("expected a candidate missing a declared key to be rejected")
	}
}

func TestNewMaterializedObjectRejectsAnExtraKey(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape":         {Capability: CapabilityImplemented, Evidence: NewIntentFieldEvidence(NewAuthoredIntentEvidence("E4.Flex", true))},
		"not_in_schema": {Capability: CapabilityImplemented, Evidence: NewObservationFieldEvidence(conformance.CoverageObserved, true)},
	}
	if _, err := NewMaterializedObject(fields, keySet("shape")); err == nil {
		t.Error("expected a candidate carrying an undeclared key to be rejected")
	}
}

func TestNewMaterializedObjectAcceptsTheEmptyObject(t *testing.T) {
	obj, err := NewMaterializedObject(map[string]MaterializedField{}, keySet())
	if err != nil {
		t.Fatalf("expected zero declared keys and zero fields to be a legitimate match, got %v", err)
	}
	if obj.Len() != 0 {
		t.Errorf("got Len() %d, want 0", obj.Len())
	}
}

// Review-caught on PR #45: the key set alone is not enough -- a Go
// zero-value field (Capability "" and FieldEvidence{} with kind "")
// passed the original key-set-only check, even though neither is a
// state any exported constructor can actually produce.
func TestNewMaterializedObjectRejectsAZeroValueField(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape": {},
	}
	if _, err := NewMaterializedObject(fields, keySet("shape")); err == nil {
		t.Error("expected a Go zero-value field to be rejected")
	}
}

// Review-caught on PR #45: Go's interface{} accepts any type, unlike
// Rust's closed Value enum -- a value outside canonical.IsValue's own
// domain must be rejected at this boundary too, not just by whatever
// later consumer happens to call canonical.ToCanonicalJSON.
func TestNewMaterializedObjectRejectsANonCICValue(t *testing.T) {
	type notACICValue struct{}
	fields := map[string]MaterializedField{
		"shape": {
			Capability: CapabilityImplemented,
			Evidence:   NewIntentFieldEvidence(NewAuthoredIntentEvidence(notACICValue{}, true)),
		},
	}
	if _, err := NewMaterializedObject(fields, keySet("shape")); err == nil {
		t.Error("expected a value outside canonical.IsValue's domain to be rejected")
	}
}

// canonical.IsValue(nil) is true -- nil is CIC's own Value::Null, a
// legitimate value in its own right, not an absence marker here (that
// distinction is hasValue's own job). Pinned down directly so the
// non-CIC-value rejection above is never mistaken for a blanket "no
// nils allowed" rule.
func TestNewMaterializedObjectAcceptsALegitimateNilValue(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape": {
			Capability: CapabilityImplemented,
			Evidence:   NewIntentFieldEvidence(NewAuthoredIntentEvidence(nil, true)),
		},
	}
	obj, err := NewMaterializedObject(fields, keySet("shape"))
	if err != nil {
		t.Fatalf("expected a legitimate nil value (hasValue=true) to be accepted, got %v", err)
	}
	f, _ := obj.Get("shape")
	v, ok := f.Value()
	if !ok || v != nil {
		t.Errorf("got (%v, %v), want (nil, true)", v, ok)
	}
}

// Review-caught on PR #45, the deeper of the two blockers: Go has no
// ownership transfer, so storing the caller's own map (the first
// draft's own MaterializedObject{fields: fields}) left the object
// mutable through the caller's still-held reference, at every level
// of nesting -- not just the top-level map, but a map/slice value
// nested arbitrarily deep inside an interface{}. Mutates BOTH the
// original top-level map AND a nested map after construction, and
// asserts neither is visible through the returned object -- the exact
// scenario the review's own example used.
func TestNewMaterializedObjectSnapshotsAgainstLaterMutation(t *testing.T) {
	nested := map[string]interface{}{"x": 1}
	fields := map[string]MaterializedField{
		"shape": {
			Capability: CapabilityImplemented,
			Evidence:   NewObservationFieldEvidence(conformance.CoverageObserved, nested),
		},
	}
	obj, err := NewMaterializedObject(fields, keySet("shape"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	fields["shape"] = MaterializedField{}
	nested["x"] = "mutated"

	f, ok := obj.Get("shape")
	if !ok {
		t.Fatal("expected \"shape\" to still be present, unaffected by the mutation of the original top-level map")
	}
	v, ok := f.Value()
	if !ok {
		t.Fatal("expected a value to still be present")
	}
	m, ok := v.(map[string]interface{})
	if !ok {
		t.Fatalf("got %T, want map[string]interface{}", v)
	}
	if m["x"] != 1 {
		t.Errorf("got %v, want 1 -- the object's own copy must be unaffected by the caller's later mutation of the nested map", m["x"])
	}
}

// Review follow-up on PR #45: Get must be just as independent on the
// way out as NewMaterializedObject is on the way in. Mutates the value
// from a FIRST Get call, then asserts a SECOND Get of the same key is
// unaffected -- proving Get returns a fresh copy each time, not an
// alias into the object's own stored state.
func TestGetReturnsAnIndependentCopyEachTime(t *testing.T) {
	fields := map[string]MaterializedField{
		"shape": {
			Capability: CapabilityImplemented,
			Evidence:   NewObservationFieldEvidence(conformance.CoverageObserved, map[string]interface{}{"x": 1}),
		},
	}
	obj, err := NewMaterializedObject(fields, keySet("shape"))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	f1, ok := obj.Get("shape")
	if !ok {
		t.Fatal("expected \"shape\" to be present")
	}
	v1, _ := f1.Value()
	m1, ok := v1.(map[string]interface{})
	if !ok {
		t.Fatalf("got %T, want map[string]interface{}", v1)
	}
	m1["x"] = "mutated by the caller, through the first Get's own result"

	f2, ok := obj.Get("shape")
	if !ok {
		t.Fatal("expected \"shape\" to still be present")
	}
	v2, _ := f2.Value()
	m2, ok := v2.(map[string]interface{})
	if !ok {
		t.Fatalf("got %T, want map[string]interface{}", v2)
	}
	if m2["x"] != 1 {
		t.Errorf("got %v, want 1 -- mutating one Get() result must not be visible through another", m2["x"])
	}
}
