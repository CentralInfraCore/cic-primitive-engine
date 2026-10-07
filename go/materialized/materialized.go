// Package materialized is the Go peer of this repo's own
// engine/src/materialized.rs: docs/MATERIALIZATION-SPEC.md's B1-B8
// (CLOSED), the custody boundary engine/src/lib.rs's "Division of
// labor" scope correction named, Rust built, and Rust wired
// (plan::evaluate's own root, PR #41) -- before this package, Go had
// no equivalent at all, a named asymmetry (README.md), not a defect.
//
// # FieldEvidence's three legitimate shapes, closed the same way ConsumedField already is
//
// B3's own prose -- "a field materialized from plain authored intent
// still carries a provenance and no coverage; a field materialized
// from a raw, non-derived observation still carries a coverage and no
// provenance... only the derived case populates both" -- names exactly
// three legitimate shapes, not a free combination of capability/
// coverage/provenance. FieldEvidence closes that the same way this
// module's own IntentEvidence closes B1's provenance-vs-value
// constraint, and the way go/plan's ConsumedField (F13) already closes
// coverage-vs-value: unexported fields plus one constructor per
// legitimate state, not a struct whose fields a caller could set into
// a combination the Rust enum would refuse to type-check.
//
// # Why this package does not import go/plan's ConsumedField
//
// materialized.rs's own FieldEvidence::Observation/DerivedObservation
// reuse plan.rs's ConsumedField directly -- both live in the same Rust
// crate, so there is no cycle to avoid. Go packages cannot do the same:
// go/plan already depends on go/conformance, and a future Go peer of
// plan::evaluate's own MaterializedObject wiring (PR #41's Go
// counterpart, not done here) would need go/plan to depend on this
// package -- which this package importing go/plan for ConsumedField
// would turn into an import cycle. So FieldEvidence reimplements the
// identical (coverage, value) state machine inline instead, using
// go/conformance's Coverage directly (the one dependency both
// go/plan's ConsumedField and this package's FieldEvidence already
// share) rather than go/plan's own wrapper around it. A named
// structural divergence from the Rust side, not an oversight, and not
// yet exercised by a cross-language differential corpus -- see this
// repo's own conformance/differential/README.md for why that is a
// separate, later layer (same relationship F6-F9 had to their own Go
// peers before F15 cross-checked them).
//
// # What this package does not do
//
// Enforce the Complete-property/key-set check against anything but the
// expectedKeys the caller hands NewMaterializedObject -- this package
// does not know what a schema's key set IS (domain-semantic knowledge,
// the environment's job per the scope correction); it only enforces
// that a candidate matches whatever key set it was handed, exactly
// mirroring MaterializedObject::try_new's own stated scope.
//
// # Review-caught on PR #45: Rust's type system closes two things Go's own does not
//
// The first draft of NewMaterializedObject checked only the key set,
// leaving two gaps a Rust caller cannot even express: (1) a Go zero
// value (MaterializedField{}, Capability "" and FieldEvidence{} with
// kind "") passes straight through unless something actually calls
// Capability.Valid()/FieldEvidence.Valid() at the boundary -- having
// those methods is not the same as using them, and the first draft
// defined them without ever calling them from the one place a
// candidate's validity is supposed to be settled; (2) Go's
// interface{}, unlike Rust's closed Value enum, accepts ANY type, so a
// value needs canonical.IsValue's own boundary check (that package's
// own doc comment already says this explicitly) before this package
// can claim the same closed value-tree shape Rust gets from its type
// system alone. NewMaterializedObject now checks both, for every
// field, before accepting a candidate.
//
// # Review-caught on PR #45: Go has no ownership to transfer, so this package copies instead
//
// MaterializedObject::try_new MOVES its BTreeMap argument in Rust --
// the caller's own map is gone afterward, enforced by the borrow
// checker at compile time, so a later mutation through the caller's
// own variable is not a program that compiles at all. Go has no such
// mechanism: a map and the []interface{}/map[string]interface{}
// nested arbitrarily inside its interface{} values are all reference
// types, so merely storing the caller's map (the first draft's own
// `MaterializedObject{fields: fields}`) left the object's own
// custody-checked state mutable through the caller's still-held
// reference, at every level of nesting, not just the top one.
// NewMaterializedObject now builds its own copy via deepCopyValue,
// safe to call unconditionally only because it always runs AFTER
// canonical.IsValue has already confirmed each value's shape is
// closed -- deepCopyValue is not a general "copy anything" utility,
// it is exhaustive against that already-validated domain.
package materialized

import (
	"fmt"
	"sort"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
)

// Capability is B1's capability axis -- access.yaml's own conformance
// field, named for the axis (B1) rather than the schema field, to keep
// it apart from go/conformance's own F6-F9 comparator vocabulary,
// which is a different "conformance" entirely. The Go peer of
// materialized.rs's own Capability enum.
type Capability string

const (
	CapabilityImplemented    Capability = "implemented"
	CapabilityNotImplemented Capability = "not_implemented"
	CapabilityDeprecated     Capability = "deprecated"
)

// Valid reports whether c is one of the three values B1 defines.
func (c Capability) Valid() bool {
	switch c {
	case CapabilityImplemented, CapabilityNotImplemented, CapabilityDeprecated:
		return true
	default:
		return false
	}
}

// Provenance is B1's provenance axis -- intent-side only. See
// IntentEvidence's own doc comment for why a state-side derived field
// never stores this directly. The Go peer of materialized.rs's own
// Provenance enum.
type Provenance string

const (
	ProvenanceAuthored      Provenance = "authored"
	ProvenanceSchemaDefault Provenance = "schema_default"
	ProvenanceDerived       Provenance = "derived"
)

// Valid reports whether p is one of the three values B1 defines.
func (p Provenance) Valid() bool {
	switch p {
	case ProvenanceAuthored, ProvenanceSchemaDefault, ProvenanceDerived:
		return true
	default:
		return false
	}
}

// IntentEvidence is intent-side evidence, one constructor per
// Provenance value, so only Authored can legitimately hold no value --
// the Go peer of materialized.rs's own IntentEvidence enum (B8:
// SchemaDefault/Derived always carry a real value -- B1's own wording,
// "substituted"/"computed", names an operation that always produces
// one -- only Authored's B2 authored-absent question stays open).
//
// Go's own nil cannot distinguish "authored, no value" from "authored,
// a legitimate null value": go/canonical's own IsValue treats nil as a
// representable CIC value in its own right (CIC's Value::Null), the
// same overload go/plan's ConsumedField.Value() already has to resolve
// with a second, explicit bool return rather than inspecting nilness
// alone. hasValue is that same explicit flag here.
type IntentEvidence struct {
	provenance Provenance
	value      interface{}
	hasValue   bool
}

// NewAuthoredIntentEvidence constructs Authored evidence. hasValue
// distinguishes "authored, but no value" (B2's still-open
// authored-absent question -- deliberately representable here, not
// resolved) from "authored with a legitimate null value."
func NewAuthoredIntentEvidence(value interface{}, hasValue bool) IntentEvidence {
	if !hasValue {
		value = nil
	}
	return IntentEvidence{provenance: ProvenanceAuthored, value: value, hasValue: hasValue}
}

// NewSchemaDefaultIntentEvidence constructs SchemaDefault evidence --
// always a real, substituted value (B1: "substituted from the
// schema's declared default"). There is no hasValue parameter: this
// state cannot be constructed without one.
func NewSchemaDefaultIntentEvidence(value interface{}) IntentEvidence {
	return IntentEvidence{provenance: ProvenanceSchemaDefault, value: value, hasValue: true}
}

// NewDerivedIntentEvidence constructs Derived evidence -- always a
// real, computed value (B1: "computed by the engine from other
// field(s)"). There is no hasValue parameter: this state cannot be
// constructed without one.
func NewDerivedIntentEvidence(value interface{}) IntentEvidence {
	return IntentEvidence{provenance: ProvenanceDerived, value: value, hasValue: true}
}

// Provenance returns the Provenance this evidence corresponds to --
// mirrors materialized.rs's own IntentEvidence::provenance.
func (ie IntentEvidence) Provenance() Provenance {
	return ie.provenance
}

// Value returns the intent-side value and whether one is actually
// present.
func (ie IntentEvidence) Value() (interface{}, bool) {
	return ie.value, ie.hasValue
}

// Valid reports whether ie is one of the states the exported
// constructors can actually produce -- catches Go's zero value
// (IntentEvidence{}, provenance "") the same way go/plan's
// ConsumedField.Valid already catches its own zero value (F13/F14's
// review-caught lesson, applied here from the start rather than found
// by a second review round), AND (review-caught on PR #45) that any
// value actually present is one canonical.IsValue recognizes -- Go's
// interface{} accepts any type, unlike Rust's closed Value enum, so
// this is the one place that domain gets closed for this type, the
// same way canonical.IsValue's own doc comment requires of every
// boundary that accepts an interface{} value.
func (ie IntentEvidence) Valid() bool {
	switch ie.provenance {
	case ProvenanceAuthored:
		if !ie.hasValue {
			return true
		}
		return canonical.IsValue(ie.value)
	case ProvenanceSchemaDefault, ProvenanceDerived:
		return ie.hasValue && canonical.IsValue(ie.value)
	default:
		return false
	}
}

// evidenceKind distinguishes FieldEvidence's three legitimate shapes
// -- unexported, the same "one constructor per state" closure
// IntentEvidence and go/plan's ConsumedField already use.
type evidenceKind string

const (
	evidenceIntent             evidenceKind = "intent"
	evidenceObservation        evidenceKind = "observation"
	evidenceDerivedObservation evidenceKind = "derived_observation"
)

// FieldEvidence is B3's three legitimate field shapes, closed so a
// fourth, illegitimate one cannot be constructed -- the Go peer of
// materialized.rs's own FieldEvidence enum. See the package doc
// comment for why this reimplements, rather than imports, go/plan's
// ConsumedField state machine for the Observation/DerivedObservation
// cases.
type FieldEvidence struct {
	kind     evidenceKind
	intent   IntentEvidence
	coverage conformance.Coverage
	value    interface{}
	hasValue bool
}

// NewIntentFieldEvidence wraps intent-side evidence. Never carries a
// Coverage -- coverage is an observe-call concept, and an intent field
// is not observed.
func NewIntentFieldEvidence(ie IntentEvidence) FieldEvidence {
	return FieldEvidence{kind: evidenceIntent, intent: ie}
}

// NewObservationFieldEvidence wraps a raw, non-derived observation.
// Mirrors go/plan's own fromCoverage: value is retained only when
// coverage is CoverageObserved, enforced here at construction rather
// than left to a later Valid() check alone -- the same "forbidden
// pairing, constructible anyway" defect PR #29/F13 already fixed once
// is closed off at the one place an externally-supplied Coverage
// enters this type.
//
// Panics if coverage is not one of Coverage's four defined values --
// the same boundary check go/plan's own fromCoverage applies.
func NewObservationFieldEvidence(coverage conformance.Coverage, value interface{}) FieldEvidence {
	return newObservationLikeFieldEvidence(evidenceObservation, coverage, value)
}

// NewDerivedObservationFieldEvidence wraps a state-side derived
// observation (BOUNDARY.md's $.state.effective_state example).
// Provenance is always exactly Derived here -- see materialized.rs's
// own doc comment for why this variant stores no separate Provenance
// field at all: naming the fact in the constructor leaves nothing to
// get wrong. Panics if coverage is not one of Coverage's four defined
// values, same as NewObservationFieldEvidence.
func NewDerivedObservationFieldEvidence(coverage conformance.Coverage, value interface{}) FieldEvidence {
	return newObservationLikeFieldEvidence(evidenceDerivedObservation, coverage, value)
}

// newObservationLikeFieldEvidence applies fromCoverage's own
// value-retention rule and validates coverage, shared by both
// observation constructors above.
func newObservationLikeFieldEvidence(kind evidenceKind, coverage conformance.Coverage, value interface{}) FieldEvidence {
	switch coverage {
	case conformance.CoverageObserved:
		return FieldEvidence{kind: kind, coverage: coverage, value: value, hasValue: true}
	case conformance.CoverageAbsent, conformance.CoverageUnobserved, conformance.CoverageUnknown:
		return FieldEvidence{kind: kind, coverage: coverage}
	default:
		panic(fmt.Sprintf("materialized: invalid Coverage %q", string(coverage)))
	}
}

// Value returns the field's materialized value, if it has one --
// false for Absent/Unobserved/Unknown coverage, for an authored-absent
// intent field (B2, still open), and for nothing else. Derived from
// the underlying evidence, not a second, independently settable
// field.
func (fe FieldEvidence) Value() (interface{}, bool) {
	switch fe.kind {
	case evidenceIntent:
		return fe.intent.Value()
	case evidenceObservation, evidenceDerivedObservation:
		return fe.value, fe.hasValue
	default:
		return nil, false
	}
}

// Valid reports whether fe is one of the states the exported
// constructors can actually produce -- catches Go's zero value
// (FieldEvidence{}, kind "") the same way IntentEvidence.Valid and
// go/plan's ConsumedField.Valid catch theirs, AND (review-caught on PR
// #45, same reasoning as IntentEvidence.Valid) that any value actually
// present is one canonical.IsValue recognizes.
func (fe FieldEvidence) Valid() bool {
	switch fe.kind {
	case evidenceIntent:
		return fe.intent.Valid()
	case evidenceObservation, evidenceDerivedObservation:
		if !fe.coverage.Valid() {
			return false
		}
		if fe.coverage == conformance.CoverageObserved {
			return fe.hasValue && canonical.IsValue(fe.value)
		}
		return !fe.hasValue
	default:
		return false
	}
}

// deepCopy returns a structural copy of ie, independent of whatever
// mutable value the caller goes on to hold -- see deepCopyValue's own
// doc comment for why this is safe to call unconditionally only after
// Valid() has already confirmed ie's value (if any) is in
// canonical.IsValue's closed domain.
func (ie IntentEvidence) deepCopy() IntentEvidence {
	ie.value = deepCopyValue(ie.value)
	return ie
}

// deepCopy returns a structural copy of fe -- see IntentEvidence's own
// deepCopy and deepCopyValue's doc comment.
func (fe FieldEvidence) deepCopy() FieldEvidence {
	switch fe.kind {
	case evidenceIntent:
		fe.intent = fe.intent.deepCopy()
	case evidenceObservation, evidenceDerivedObservation:
		fe.value = deepCopyValue(fe.value)
	}
	return fe
}

// deepCopyValue returns a structural copy of v, so a MaterializedObject
// stays independent of whatever mutable map/slice the caller goes on to
// hold after construction -- review-caught on PR #45: Go's map and
// slice are reference types, nested arbitrarily deep inside an
// interface{} value, so copying only NewMaterializedObject's own
// top-level fields map is not enough on its own to give this package
// anything like Rust's ownership-transfer guarantee.
//
// Safe to call unconditionally only because every call site here runs
// AFTER canonical.IsValue has already confirmed v's shape is closed
// (via Valid()) -- this is not a general "copy anything" utility, it
// is exhaustive against that already-validated domain, the same
// closed value-tree IsValue's own doc comment defines.
func deepCopyValue(v interface{}) interface{} {
	switch vv := v.(type) {
	case map[string]interface{}:
		out := make(map[string]interface{}, len(vv))
		for k, elem := range vv {
			out[k] = deepCopyValue(elem)
		}
		return out
	case []interface{}:
		out := make([]interface{}, len(vv))
		for i, elem := range vv {
			out[i] = deepCopyValue(elem)
		}
		return out
	default:
		// nil, bool, string, every numeric kind, json.Number -- all
		// immutable in Go; no copy needed.
		return v
	}
}

// MaterializedField is B3's closed MaterializedField shape: Capability
// (B1, always present -- a static per-field/device-binding fact) plus
// FieldEvidence (the coverage/provenance/value facts, whose legal
// combinations FieldEvidence's own constructors already close) -- the
// Go peer of materialized.rs's own MaterializedField.
type MaterializedField struct {
	Capability Capability
	Evidence   FieldEvidence
}

// Value returns the field's materialized value, if it has one -- see
// FieldEvidence.Value.
func (f MaterializedField) Value() (interface{}, bool) {
	return f.Evidence.Value()
}

// MaterializedObject is the B7/F13-F14 custody-boundary obligation
// engine/src/lib.rs's "Division of labor" scope correction named:
// exactly the schema's keys, no more and no less (PRIMITIVE-IR.md's
// own Complete property), enforced by construction -- the Go peer of
// materialized.rs's own MaterializedObject.
type MaterializedObject struct {
	fields map[string]MaterializedField
}

// NewMaterializedObject accepts fields as a genuine MaterializedObject
// only if ALL of the following hold: its key set is EXACTLY
// expectedKeys (not a subset, not a superset); every field's
// Capability and FieldEvidence are Valid() (catching both a Go zero
// value and a value outside canonical.IsValue's closed domain --
// review-caught on PR #45, see the package doc comment's own two
// review-caught notes). expectedKeys is the caller's (the
// environment's) own claim about what the schema declares; this
// package does not independently verify that claim against any schema
// (out of scope, per the scope correction), only that the candidate
// actually matches whatever claim it was handed. The returned object
// holds its own deep copy of fields, not the caller's own map or any
// value nested inside it -- a later mutation of the argument is never
// visible through the returned MaterializedObject. The Go peer of
// materialized.rs's own MaterializedObject::try_new -- Rust gets this
// last guarantee from ownership transfer at compile time; this is
// Go's own, explicit substitute for it.
func NewMaterializedObject(
	fields map[string]MaterializedField,
	expectedKeys map[string]struct{},
) (MaterializedObject, error) {
	var missing, extra []string
	for k := range expectedKeys {
		if _, ok := fields[k]; !ok {
			missing = append(missing, k)
		}
	}
	for k := range fields {
		if _, ok := expectedKeys[k]; !ok {
			extra = append(extra, k)
		}
	}
	if len(missing) > 0 || len(extra) > 0 {
		sort.Strings(missing)
		sort.Strings(extra)
		return MaterializedObject{}, fmt.Errorf(
			"materialized: key set does not match: missing=%v, extra=%v", missing, extra,
		)
	}

	copied := make(map[string]MaterializedField, len(fields))
	for k, f := range fields {
		if !f.Capability.Valid() {
			return MaterializedObject{}, fmt.Errorf(
				"materialized: field %q has an invalid Capability %q", k, string(f.Capability),
			)
		}
		if !f.Evidence.Valid() {
			return MaterializedObject{}, fmt.Errorf(
				"materialized: field %q has an invalid FieldEvidence", k,
			)
		}
		copied[k] = MaterializedField{Capability: f.Capability, Evidence: f.Evidence.deepCopy()}
	}
	return MaterializedObject{fields: copied}, nil
}

// Get returns the field at key, if present.
//
// Named, not fixed here: the value Get (via MaterializedField.Value)
// hands back is the object's own internal copy, not a further copy of
// it -- a caller who mutates a returned map/slice value in place would
// corrupt o's own state for a later Get of the same key. Rust's
// `.value()` returns an immutable `&Value` borrow, which the borrow
// checker already prevents from being mutated through; Go has no
// equivalent on the read side either. Out of scope for the
// construction-boundary fix this PR makes (review's own two blockers
// were both about NewMaterializedObject, not Get), named so it is not
// mistaken for something this package already closes.
func (o MaterializedObject) Get(key string) (MaterializedField, bool) {
	f, ok := o.fields[key]
	return f, ok
}

// Len returns the number of fields in o.
func (o MaterializedObject) Len() int {
	return len(o.fields)
}
