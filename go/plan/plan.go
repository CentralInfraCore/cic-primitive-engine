// Package plan implements the object-level comparator walk --
// docs/MATERIALIZATION-SPEC.md's section F -- the Go peer of this
// repo's own engine/src/plan.rs (F8). Both ultimately trace to the
// same source, CIC-Relay's core/nexus/iac/conformance.go
// (ConformancePlan/FieldPlan/CollectionPlan/Evaluate/aggregate/
// elementKeys/resolvePath). Closes F1's last open item on the Go side
// too: F6's per-field primitives (go/conformance) and F7's
// element-identity primitive (go/collection) exist; this is what
// drives them over a whole document.
//
// # A naming note, mirroring plan.rs's own
//
// This repo's own conformance.rs/go/conformance port Go's
// compare.go/observation.go (the per-field primitives) -- NOT
// conformance.go, despite the name. conformance.go's own content --
// the object-level walk -- lives here, in plan.go (and, on the Rust
// side, plan.rs), named for what it actually is.
//
// # resolvePath fixes the same inherited multi-key bug plan.rs already fixed
//
// Real Go's own resolvePath (conformance.go) splits a "{...}" segment
// on only the FIRST "=" (strings.Cut), which breaks for the multi-key
// identity go/collection's ElementKey already builds
// ("name=nic-0,zone=eu") -- an inherited Relay bug, not a Go<->Rust
// divergence, found and fixed on the Rust side on PR #28. Per this
// repo's standing A0 policy (CIC-Relay is migration source and tested
// reference material, not a contract to reproduce bug-for-bug), this
// package peers against plan.rs's FIXED behavior, not conformance.go's
// original: a "{...}" segment is parsed as the comma-separated "k=v"
// list ElementKey itself builds, every pair matched via
// collection.GoDisplay (reused, not a second copy), and all pairs must
// match. A single-key identity is simply the one-constraint case of
// this, so the common case is unaffected.
//
// # What this does not do
//
// Produce conformance_plan_digest/observation_digest themselves (the
// Go peer of digest_projection.rs, F9, not yet written) -- Evaluate's
// own ObjectVerdict.Consumed records exactly what that step will need,
// the same way plan.rs's own ObjectVerdict.consumed does.
package plan

import (
	"fmt"
	"sort"
	"strings"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
	"github.com/CentralInfraCore/cic-primitive-engine/go/collection"
	"github.com/CentralInfraCore/cic-primitive-engine/go/conformance"
)

// FieldPlan binds one comparable field path to its comparator -- the
// Go peer of plan.rs's own FieldPlan. Path is canonical ("/shape", or
// relative within a collection element, e.g. "subnet").
type FieldPlan struct {
	Path    string
	Compare conformance.CompareType
}

// CollectionPlan is a topology-identified list within the object, and
// the per-element field plans (paths relative to the element) -- the
// Go peer of plan.rs's own CollectionPlan.
type CollectionPlan struct {
	Path       string
	Collection collection.Collection
	Elements   []FieldPlan
}

// ConformancePlan is the compiled comparison plan for an object: flat
// scalar fields plus topology-identified collections -- the Go peer
// of plan.rs's own ConformancePlan.
type ConformancePlan struct {
	Scalars     []FieldPlan
	Collections []CollectionPlan
}

// Validate panics unless every FieldPlan's CompareType and every
// CollectionPlan's CollectionTopology is one of the values that
// type's own Valid() recognizes.
//
// Review-caught on PR #33: calling Compare/ElementKey with an invalid
// CompareType/CollectionTopology already panics (PR #31/#32), but
// whether that call actually HAPPENS for a given field depends on
// execution path, not on the plan being well-formed.
// ClassifyFieldValue never calls Compare at all unless coverage is
// Observed, so FieldPlan{Compare: CompareType("garbage")} paired with
// an Unobserved field silently verdicts UNOBSERVED, never panicking;
// a CollectionPlan whose Path never resolves to any elements never
// calls ElementKey at all, so CollectionTopology("garbage") never
// surfaces either. Both reproduced directly before this fix: a plan
// built with either garbage value produced an ordinary-looking
// ObjectVerdict with no panic, no error, nothing -- the exact "the
// type system can't close this off, so the boundary must" gap PR
// #31/#32 already closed for Coverage/CompareType/CollectionTopology's
// own call sites, just one level up: a malformed PLAN, not a single
// malformed call, needs the identical check at Evaluate's own
// boundary, not deferred to whichever paths execution happens to
// take.
func (cp ConformancePlan) Validate() {
	for _, fp := range cp.Scalars {
		if !fp.Compare.Valid() {
			panic(fmt.Sprintf("plan: invalid CompareType %q in scalar field plan %q", string(fp.Compare), fp.Path))
		}
	}
	for _, coll := range cp.Collections {
		if !coll.Collection.Topology.Valid() {
			panic(fmt.Sprintf("plan: invalid CollectionTopology %q in collection plan %q", string(coll.Collection.Topology), coll.Path))
		}
		for _, fp := range coll.Elements {
			if !fp.Compare.Valid() {
				panic(fmt.Sprintf("plan: invalid CompareType %q in element field plan %q of collection %q", string(fp.Compare), fp.Path, coll.Path))
			}
		}
	}
}

// ObjectConformance is the aggregate object verdict -- identical
// string constants to conformance.go's own ObjectConformance.
//
// Unlike Coverage/CompareType/CollectionTopology, nothing in this
// package ever receives an ObjectConformance as an external,
// untrusted parameter -- it is only ever produced by aggregate, whose
// own switch already returns one of the three constants below, by
// construction. So there is no Valid()+panic guard here: that guard
// earns its keep at a boundary that accepts arbitrary input, and this
// type has none.
type ObjectConformance string

const (
	// ObjectConformant -- every planned field is CONFORMANT or OBSERVED_ABSENT.
	ObjectConformant ObjectConformance = "CONFORMANT"
	// ObjectDrift -- at least one field is DRIFT or NOT_COMPARABLE.
	ObjectDrift ObjectConformance = "DRIFT"
	// ObjectIncomplete -- no drift, but at least one planned field was
	// UNOBSERVED, so full conformance cannot be claimed.
	ObjectIncomplete ObjectConformance = "INCOMPLETE"
)

// ConsumedField is one path's contribution to the future F9 Go peer's
// ObservationDigestProjection -- Coverage paired with a value EXACTLY
// when that pairing is legitimate (conformance.ClassifyFieldValue
// never reads a value for any coverage state but Observed, so there
// is nothing a non-Observed entry could truthfully carry). The Go
// peer of plan.rs's own ConsumedField enum (Observed(Value) | Absent |
// Unobserved | Unknown).
//
// Go has no closed sum type, so this uses Go's actual closest
// equivalent instead of the Valid()+panic pattern Coverage/
// CompareType/CollectionTopology use: unexported fields plus one
// constructor PER STATE (NewObservedConsumedField/
// NewAbsentConsumedField/NewUnobservedConsumedField/
// NewUnknownConsumedField), mirroring the Rust enum's own four
// variants one-for-one, rather than a single two-argument constructor
// that accepts a (coverage, value) pair and decides what to do with
// it.
//
// Review-caught on PR #33: an earlier version had exactly that single
// two-argument constructor (NewConsumedField(coverage, observedValue)),
// which DID reject an invalid Coverage, but still silently ACCEPTED
// and discarded observedValue for every non-Observed coverage --
// NewConsumedField(CoverageAbsent, "I should not exist") built a
// valid-looking ConsumedField with no error, the exact "forbidden
// pairing silently accepted" shape PR #29 already named a defect on
// the Rust side (the value's own origin didn't even reach the type
// invariant check; it was just dropped on the floor). Splitting into
// per-state constructors removes the parameter entirely from the
// three states that must never carry one -- there is no argument
// position left for a caller to mistakenly believe survives into
// Absent/Unobserved/Unknown.
//
// Code OUTSIDE this package cannot construct the forbidden pairing at
// all -- there is no exported way to build a ConsumedField except
// through these constructors. This is a narrower guarantee than
// Rust's enum, which closes off even code INSIDE the same module --
// recorded honestly, not overstated, since Go code within this very
// package could still write the struct literal directly if it tried.
type ConsumedField struct {
	coverage conformance.Coverage
	value    interface{}
}

// NewObservedConsumedField is the only way to construct a
// ConsumedField whose Value() returns a value.
func NewObservedConsumedField(observedValue interface{}) ConsumedField {
	return ConsumedField{coverage: conformance.CoverageObserved, value: observedValue}
}

// NewAbsentConsumedField constructs a ConsumedField for a path the
// observation authoritatively reported as absent.
func NewAbsentConsumedField() ConsumedField {
	return ConsumedField{coverage: conformance.CoverageAbsent}
}

// NewUnobservedConsumedField constructs a ConsumedField for a path
// the observation never covered.
func NewUnobservedConsumedField() ConsumedField {
	return ConsumedField{coverage: conformance.CoverageUnobserved}
}

// NewUnknownConsumedField constructs a ConsumedField for a path whose
// observed value was indeterminate (B2's own addition, no Relay
// equivalent).
func NewUnknownConsumedField() ConsumedField {
	return ConsumedField{coverage: conformance.CoverageUnknown}
}

// fromCoverage is classifyAt's own internal constructor, mirroring
// plan.rs's own (also non-exported) ConsumedField::from_coverage: it
// is the one place a runtime Coverage value -- not a caller who
// already knows which state they mean -- decides which of the four
// per-state constructors to call, and it is the only place in this
// package allowed to do so. Not exported, so nothing outside this
// file can reintroduce the two-argument shape the review caught.
// Panics if coverage is not one of Coverage's four defined values --
// the same boundary check Coverage.Valid's own doc comment describes,
// applied here because this is where an externally-supplied Coverage
// (obs.Coverage(path)'s return value) enters this type.
func fromCoverage(coverage conformance.Coverage, observedValue interface{}) ConsumedField {
	switch coverage {
	case conformance.CoverageObserved:
		return NewObservedConsumedField(observedValue)
	case conformance.CoverageAbsent:
		return NewAbsentConsumedField()
	case conformance.CoverageUnobserved:
		return NewUnobservedConsumedField()
	case conformance.CoverageUnknown:
		return NewUnknownConsumedField()
	default:
		panic(fmt.Sprintf("plan: invalid Coverage %q", string(coverage)))
	}
}

// Coverage returns the recorded coverage.
func (c ConsumedField) Coverage() conformance.Coverage {
	return c.coverage
}

// Value returns the observed value and true iff Coverage() ==
// conformance.CoverageObserved.
func (c ConsumedField) Value() (interface{}, bool) {
	if c.coverage != conformance.CoverageObserved {
		return nil, false
	}
	return c.value, true
}

// Valid reports whether c is one of the four states the exported
// constructors (or fromCoverage) can actually produce.
//
// Review-caught on PR #34: closing off the public CONSTRUCTORS (PR
// #33's own fix) does not close off Go's zero value, which exists for
// every struct type regardless of whether any constructor was ever
// called. var zero plan.ConsumedField compiles to {coverage: "",
// value: nil} with no error -- zero.Coverage() is the empty string,
// not a rejection -- and nothing stopped it from flowing straight into
// ObservationDigestProjection's output ({"path": p, "coverage": ""}),
// which canonical.ToCanonicalJSON then digests without complaint: a
// SHA-256 of an invalid semantic state, the exact "invalid state
// type-checks, canonicalizes and digests without complaint" shape PR
// #29 already named a defect, reached this time through Go's zero
// value rather than through either a struct literal or a constructor
// argument. Per-state constructors close off the FIRST category (a
// wrong coverage/value combination); they cannot close off the
// SECOND (never having called a constructor at all) -- only an
// explicit check at the consuming boundary can, which is what this
// method is for.
func (c ConsumedField) Valid() bool {
	switch c.coverage {
	case conformance.CoverageObserved:
		return true
	case conformance.CoverageAbsent, conformance.CoverageUnobserved, conformance.CoverageUnknown:
		return c.value == nil
	default:
		return false
	}
}

// ObjectVerdict is the object-level conformance verdict -- the Go
// peer of plan.rs's own ObjectVerdict, minus IntentDigest/
// ObservationDigest themselves (the future F9 Go peer's job). Fields
// and Consumed are plain Go maps, not a sorted structure: Go has no
// stdlib BTreeMap, and nothing in this package itself needs sorted
// iteration -- a future caller that does (the F9 Go peer, matching
// plan.rs's own digest_projection.rs dependency) sorts keys at the
// point it actually needs them, the same way go/canonical's own
// writeMap already does, rather than this type carrying a sorted
// container everywhere just in case.
type ObjectVerdict struct {
	Object   ObjectConformance
	Fields   map[string]conformance.FieldVerdict
	Consumed map[string]ConsumedField
}

// Evaluate produces the object-level conformance verdict for intent
// against observed, under obs's coverage and the compiled plan. Only
// planned fields are compared; unplanned state fields are
// intentionally ignored (observed state, not drift) -- the Go peer of
// plan.rs's own evaluate, identical to conformance.go's own Evaluate.
//
// Panics if intent or observed is not a representable CIC value tree
// (canonical.IsValue) -- the same "the type system can't close this
// off, so the boundary must" principle PR #31/#32 already established
// for Coverage/CompareType/CollectionTopology, applied at this
// package's own entry point so every value resolvePath/
// collection.ElementKey touch downstream is already known-valid; see
// go/collection's own package doc for the bug this prevents. Also
// panics if cp itself is malformed -- see ConformancePlan.Validate's
// own doc comment for why this must be checked here, unconditionally,
// rather than left to whichever per-field call paths execution
// happens to take.
func Evaluate(
	intent, observed map[string]interface{},
	obs *conformance.Observation,
	cp ConformancePlan,
) ObjectVerdict {
	if !canonical.IsValue(intent) {
		panic("plan: intent is not a representable CIC value tree")
	}
	if !canonical.IsValue(observed) {
		panic("plan: observed is not a representable CIC value tree")
	}
	cp.Validate()

	fields := make(map[string]conformance.FieldVerdict)
	consumed := make(map[string]ConsumedField)

	for _, fp := range cp.Scalars {
		classifyAt(fields, consumed, intent, observed, obs, fp.Path, fp.Compare)
	}

	for _, coll := range cp.Collections {
		for _, ek := range elementKeys(intent, observed, coll) {
			for _, ef := range coll.Elements {
				path := coll.Path + "/{" + ek + "}/" + ef.Path
				classifyAt(fields, consumed, intent, observed, obs, path, ef.Compare)
			}
		}
	}

	return ObjectVerdict{
		Object:   aggregate(fields),
		Fields:   fields,
		Consumed: consumed,
	}
}

func classifyAt(
	fields map[string]conformance.FieldVerdict,
	consumed map[string]ConsumedField,
	intent, observed map[string]interface{},
	obs *conformance.Observation,
	path string,
	ct conformance.CompareType,
) {
	coverage := obs.Coverage(path)
	iv, iPresent := resolvePath(intent, path)
	ov, _ := resolvePath(observed, path)
	// A missing path resolves to nil here (resolvePath's own ok=false
	// case), which classifyAt simply passes through -- no Value::Null
	// placeholder needed, unlike the Rust side: Go's nil interface{}
	// already serves as the "absent" value conformance.Compare/
	// ClassifyFieldValue expect, matching the convention
	// go/collection's own "missing key field renders <nil> for free"
	// doc comment already names.
	fields[path] = conformance.ClassifyFieldValue(coverage, iPresent, iv, ov, ct)
	// F5's own rule, not invented here: a value is part of what the
	// comparator consumed iff coverage is Observed -- enforced by
	// construction via fromCoverage, not by convention.
	consumed[path] = fromCoverage(coverage, ov)
}

// aggregate reduces per-field verdicts to the object verdict --
// precedence DRIFT/NOT_COMPARABLE > UNOBSERVED > CONFORMANT, the Go
// peer of plan.rs's own aggregate, identical to conformance.go's own.
func aggregate(fields map[string]conformance.FieldVerdict) ObjectConformance {
	hasDrift, hasUnobserved := false, false
	for _, v := range fields {
		switch v {
		case conformance.VerdictDrift, conformance.VerdictNotComparable:
			hasDrift = true
		case conformance.VerdictUnobserved:
			hasUnobserved = true
		}
	}
	switch {
	case hasDrift:
		return ObjectDrift
	case hasUnobserved:
		return ObjectIncomplete
	default:
		return ObjectConformant
	}
}

// elementKeys returns the sorted union of element identities of a
// collection across the intent and observed lists, keyed by the
// collection's topology -- the Go peer of plan.rs's own element_keys,
// identical to conformance.go's own elementKeys.
func elementKeys(intent, observed map[string]interface{}, cp CollectionPlan) []string {
	seen := map[string]struct{}{}
	for _, root := range []map[string]interface{}{intent, observed} {
		v, ok := resolvePath(root, cp.Path)
		if !ok {
			continue
		}
		list, ok := v.([]interface{})
		if !ok {
			continue
		}
		for _, elem := range list {
			if ek := cp.Collection.ElementKey(elem); ek != "" {
				seen[ek] = struct{}{}
			}
		}
	}
	keys := make([]string, 0, len(seen))
	for k := range seen {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// resolvePath walks a canonical path ("/a/b", or "/list/{key=val}/field",
// or, for a multi-key collection, "/list/{k1=v1,k2=v2}/field") into a
// nested value, returning the value found and whether it was found at
// all -- the Go peer of plan.rs's own resolve_path. See the package
// doc for why this fixes, rather than ports, conformance.go's own
// single-split "{...}" bug.
//
// A "{...}" segment is only ever matched against map[string]interface{}
// elements, same as real Go's resolvePath and plan.rs's resolve_path --
// a TopologySet element (a bracketed segment with no "=") has no
// working case here either, the same named, inherited limitation
// plan.rs's own doc comment leaves unfixed because nothing exercises
// it.
func resolvePath(root interface{}, path string) (interface{}, bool) {
	cur := root
	for _, seg := range strings.Split(strings.Trim(path, "/"), "/") {
		if seg == "" {
			continue
		}
		if strings.HasPrefix(seg, "{") && strings.HasSuffix(seg, "}") {
			list, ok := cur.([]interface{})
			if !ok {
				return nil, false
			}
			inner := seg[1 : len(seg)-1]
			found, ok := findByConstraints(list, inner)
			if !ok {
				return nil, false
			}
			cur = found
			continue
		}
		m, ok := cur.(map[string]interface{})
		if !ok {
			return nil, false
		}
		next, ok := m[seg]
		if !ok {
			return nil, false
		}
		cur = next
	}
	return cur, true
}

// findByConstraints finds the element of list whose fields match
// every comma-separated "k=v" pair in constraints, in full -- the
// multi-key fix named in the package doc, ported from plan.rs's own
// resolve_path rather than conformance.go's single-pair original.
func findByConstraints(list []interface{}, constraints string) (interface{}, bool) {
	for _, elem := range list {
		m, ok := elem.(map[string]interface{})
		if !ok {
			continue
		}
		if matchesAllConstraints(m, constraints) {
			return elem, true
		}
	}
	return nil, false
}

func matchesAllConstraints(m map[string]interface{}, constraints string) bool {
	for _, part := range strings.Split(constraints, ",") {
		key, val, _ := strings.Cut(part, "=")
		display, ok := collection.GoDisplay(m[key])
		if !ok || display != val {
			return false
		}
	}
	return true
}
