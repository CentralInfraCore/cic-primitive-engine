// Package conformance implements section F's per-field comparator
// primitive (docs/MATERIALIZATION-SPEC.md) -- the Go peer of this
// repo's own engine/src/conformance.rs.
//
// # Why this is a peer, not a re-port of CIC-Relay's compare.go/observation.go
//
// engine/src/conformance.rs itself ports CIC-Relay's
// core/nexus/iac/compare.go and observation.go's
// ClassifyField/ClassifyFieldValue. This package implements the same,
// already-decided contract directly in Go rather than re-exporting
// those two files unchanged, for two reasons documented on the Rust
// side and equally true here:
//
//   - Coverage is B3's four-value axis (observed | absent | unobserved |
//     unknown), not Relay's two-list Observation{Observed,
//     AuthoritativeAbsent} envelope, which has no fourth ("unknown")
//     state and no object-level walker to feed it yet either.
//   - F8 (plan.rs, PR #28) found a real bug in Relay's own resolvePath
//     (strings.Cut only splits on the FIRST "=", breaking multi-key
//     collection identity) that was fixed on the Rust side rather than
//     carried forward -- per this repo's own migration-source policy
//     (A0), Relay is a tested reference to port FROM, not a contract to
//     reproduce bug-for-bug.
//
// # The numeric comparator calls math/big.Rat.SetString directly
//
// engine/src/conformance.rs's rat_from_string hand-ports
// big.Rat.SetString's full parsing grammar, because Rust has no access
// to that function itself. This package has no such gap to close: it
// IS Go, so it calls big.Rat.SetString directly, exactly as Relay's own
// compare.go already does. The grammar-parity vectors PR #25 spent two
// review rounds closing on the Rust side are, on this side, simply
// exercising the real stdlib function they were written to match.
package conformance

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math/big"
	"strconv"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
)

// Coverage is B3's four-value coverage axis. See the package doc for
// why this is four values, not Relay's two-list envelope.
type Coverage string

const (
	CoverageObserved   Coverage = "observed"
	CoverageAbsent     Coverage = "absent"
	CoverageUnobserved Coverage = "unobserved"
	CoverageUnknown    Coverage = "unknown"
)

// Valid reports whether c is one of the four coverage states this
// package defines. Go's type system has no closed-enum guarantee the
// way the Rust peer's Coverage enum does -- Coverage("garbage")
// compiles and type-checks here, where the equivalent would not even
// build on the Rust side. ClassifyField and Observation.Set call this
// at their own boundary and panic rather than silently computing a
// plausible-looking verdict for a value that was never a real
// Coverage -- review-caught on PR #31: an invalid Coverage used to
// fall through a switch's default case straight to UNOBSERVED,
// indistinguishable from a genuinely unobserved field. The Go peer of
// this crate's own "an invariant the type system can't enforce gets
// checked and rejected at its boundary instead" pattern (compare
// Value::BigInt's own invariant, or canonical_integer's .expect on the
// Rust side) -- a panic, not a silent fallback, because this is a
// programming error on the caller's part, not a runtime condition the
// caller can usefully recover from.
func (c Coverage) Valid() bool {
	switch c {
	case CoverageObserved, CoverageAbsent, CoverageUnobserved, CoverageUnknown:
		return true
	}
	return false
}

// Observation is a per-path coverage lookup for the object-level walker
// (the Go peer of plan.go, not yet written) -- not a port of Relay's
// Observation{Observed, AuthoritativeAbsent} two-list envelope. A path
// with no explicit entry defaults to CoverageUnobserved, mirroring
// Relay's own Observation.Coverage default case ("a path in neither set
// is unobserved").
type Observation struct {
	byPath map[string]Coverage
}

// NewObservation returns an empty Observation.
func NewObservation() *Observation {
	return &Observation{byPath: make(map[string]Coverage)}
}

// Set records path's coverage. A second call for the same path replaces
// the first -- this type makes no claim about which call "should win"
// for a path recorded twice; that is the caller's own invariant to
// hold, same as on the Rust side.
//
// Panics if c is not one of the four coverage states Coverage.Valid
// recognizes -- see that method's doc comment for why this fails
// closed instead of storing (and later silently misclassifying) a
// value that was never a real Coverage.
func (o *Observation) Set(path string, c Coverage) {
	if !c.Valid() {
		panic(fmt.Sprintf("conformance: invalid Coverage %q for path %q", string(c), path))
	}
	o.byPath[path] = c
}

// Coverage returns path's recorded coverage, or CoverageUnobserved if
// none was recorded.
func (o *Observation) Coverage(path string) Coverage {
	if c, ok := o.byPath[path]; ok {
		return c
	}
	return CoverageUnobserved
}

// CompareType is the field's behavior.compare annotation.
type CompareType string

const (
	// CompareExact -- structural canonical equality: canonical-JSON
	// encodings must be identical. Values of different JSON kinds are
	// NOT comparable.
	CompareExact CompareType = "exact"
	// CompareNumeric -- numeric equality with representation
	// normalization: 1, 1.0, 1e0 and "1" are equal. Values that are not
	// numeric on both sides are NOT comparable.
	CompareNumeric CompareType = "numeric"
)

// Valid reports whether ct is one of the two comparator types this
// package defines. See Coverage.Valid's doc comment for why Compare
// checks this and panics rather than silently falling back to one
// comparator for any unrecognized value -- review-caught on PR #31:
// Compare(..., CompareType("garbage")) used to silently run the exact
// comparator, indistinguishable from an explicit CompareExact.
func (ct CompareType) Valid() bool {
	switch ct {
	case CompareExact, CompareNumeric:
		return true
	}
	return false
}

// FieldVerdict is the per-field conformance verdict -- identical to
// observation.go's own five FieldVerdict string constants, the literal
// docs/VERDICT-SCHEMA.md's fields map commits to per path.
type FieldVerdict string

const (
	VerdictConformant     FieldVerdict = "CONFORMANT"
	VerdictDrift          FieldVerdict = "DRIFT"
	VerdictObservedAbsent FieldVerdict = "OBSERVED_ABSENT"
	VerdictUnobserved     FieldVerdict = "UNOBSERVED"
	VerdictNotComparable  FieldVerdict = "NOT_COMPARABLE"
)

// Compare answers whether observed conforms to intent under ct. It
// returns (matched, comparable) -- matched is meaningful only when
// comparable is true; the caller surfaces an incomparable pair as
// NOT_COMPARABLE, never as a false DRIFT.
//
// Panics if ct is not CompareExact or CompareNumeric -- deliberately
// not folded into the (matched, comparable) result (e.g. as a false
// "not comparable"), which would conflate a genuine NOT_COMPARABLE
// verdict (both sides well-formed, just not comparable to each other)
// with a caller passing a value that was never a real CompareType to
// begin with. See CompareType.Valid's doc comment.
func Compare(intent, observed interface{}, ct CompareType) (matched, comparable bool) {
	if !ct.Valid() {
		panic(fmt.Sprintf("conformance: invalid CompareType %q", string(ct)))
	}
	if ct == CompareNumeric {
		return compareNumeric(intent, observed)
	}
	return compareExact(intent, observed)
}

func compareNumeric(intent, observed interface{}) (matched, comparable bool) {
	a, aok := asRational(intent)
	b, bok := asRational(observed)
	if !aok || !bok {
		return false, false
	}
	return a.Cmp(b) == 0, true
}

// isNumericValue reports whether v is one of Go's native numeric kinds
// or json.Number -- never a plain string, matching the Rust side's own
// is_numeric_value (Value::Str is deliberately excluded there too).
func isNumericValue(v interface{}) bool {
	switch v.(type) {
	case int, int8, int16, int32, int64,
		uint, uint8, uint16, uint32, uint64,
		float32, float64, json.Number:
		return true
	}
	return false
}

// jsonKind names the JSON kind of a value for exact type-compatibility.
// All numeric Go kinds collapse to "number", matching Rust's json_kind
// collapsing all three of its own numeric Value variants the same way.
func jsonKind(v interface{}) string {
	switch v.(type) {
	case nil:
		return "null"
	case bool:
		return "bool"
	case int, int8, int16, int32, int64,
		uint, uint8, uint16, uint32, uint64,
		float32, float64, json.Number:
		return "number"
	case string:
		return "string"
	case []interface{}:
		return "array"
	case map[string]interface{}:
		return "object"
	default:
		return ""
	}
}

func compareExact(intent, observed interface{}) (matched, comparable bool) {
	iNum, oNum := isNumericValue(intent), isNumericValue(observed)
	if iNum || oNum {
		if iNum != oNum {
			return false, false // number vs non-number -> not comparable
		}
		in, iok := canonicalNumberString(intent)
		on, ook := canonicalNumberString(observed)
		if !iok || !ook {
			return false, false
		}
		return in == on, true
	}
	if jsonKind(intent) != jsonKind(observed) {
		return false, false
	}
	return canonicalEqual(intent, observed), true
}

// canonicalNumberString returns the canonical decimal form of a value
// already known to be numeric (isNumericValue(v) == true), reusing
// package canonical's own number canonicalization rather than a second
// implementation -- the same digits ToCanonicalJSON would write for
// this value, without re-serializing a whole tree. Returns ok=false if
// v is not actually numeric; callers here only ever call it after
// isNumericValue has confirmed it is.
func canonicalNumberString(v interface{}) (string, bool) {
	switch n := v.(type) {
	case int:
		return strconv.FormatInt(int64(n), 10), true
	case int8:
		return strconv.FormatInt(int64(n), 10), true
	case int16:
		return strconv.FormatInt(int64(n), 10), true
	case int32:
		return strconv.FormatInt(int64(n), 10), true
	case int64:
		return strconv.FormatInt(n, 10), true
	case uint:
		return strconv.FormatUint(uint64(n), 10), true
	case uint8:
		return strconv.FormatUint(uint64(n), 10), true
	case uint16:
		return strconv.FormatUint(uint64(n), 10), true
	case uint32:
		return strconv.FormatUint(uint64(n), 10), true
	case uint64:
		return strconv.FormatUint(n, 10), true
	case float32:
		return canonical.CanonicalFloat(float64(n)), true
	case float64:
		return canonical.CanonicalFloat(n), true
	case json.Number:
		if s, ok := canonical.CanonicalInteger(string(n)); ok {
			return s, true
		}
		f, err := strconv.ParseFloat(string(n), 64)
		if err != nil {
			return "", false
		}
		return canonical.CanonicalFloat(f), true
	default:
		return "", false
	}
}

// canonicalEqual reuses package canonical's tree encoder instead of a
// second serializer -- the Go peer of conformance.rs's own
// canonical_equal. A value that fails to canonicalize (a non-finite
// float, per section A's own rejection) is treated as unequal rather
// than erroring further -- it should never reach this function on an
// already-materialized value, but compareExact must stay total either
// way.
func canonicalEqual(a, b interface{}) bool {
	ab, err := canonical.ToCanonicalJSON(a)
	if err != nil {
		return false
	}
	bb, err := canonical.ToCanonicalJSON(b)
	if err != nil {
		return false
	}
	return bytes.Equal(ab, bb)
}

// asRational coerces v to an exact rational for numeric-comparator
// equality -- never a lossy float64 midpoint. Returns ok=false for
// anything not numeric-compatible, including non-finite floats
// (big.Rat.SetFloat64 returns nil for NaN/Infinity) and bool (never
// numeric). For strings and json.Number, this calls big.Rat.SetString
// directly -- see the package doc for why no hand-written grammar is
// needed here, unlike on the Rust side.
func asRational(v interface{}) (*big.Rat, bool) {
	switch n := v.(type) {
	case int:
		return new(big.Rat).SetInt64(int64(n)), true
	case int8:
		return new(big.Rat).SetInt64(int64(n)), true
	case int16:
		return new(big.Rat).SetInt64(int64(n)), true
	case int32:
		return new(big.Rat).SetInt64(int64(n)), true
	case int64:
		return new(big.Rat).SetInt64(n), true
	case uint:
		return new(big.Rat).SetUint64(uint64(n)), true
	case uint8:
		return new(big.Rat).SetUint64(uint64(n)), true
	case uint16:
		return new(big.Rat).SetUint64(uint64(n)), true
	case uint32:
		return new(big.Rat).SetUint64(uint64(n)), true
	case uint64:
		return new(big.Rat).SetUint64(n), true
	case float32:
		r := new(big.Rat)
		if r.SetFloat64(float64(n)) == nil { // nil for Inf/NaN
			return nil, false
		}
		return r, true
	case float64:
		r := new(big.Rat)
		if r.SetFloat64(n) == nil { // nil for Inf/NaN
			return nil, false
		}
		return r, true
	case json.Number:
		return new(big.Rat).SetString(n.String())
	case string:
		return new(big.Rat).SetString(n)
	default:
		return nil, false
	}
}

// ClassifyField produces a per-field verdict from coverage and the
// comparison outcome -- the Go peer of conformance.rs's classify_field,
// identical to observation.go's own ClassifyField.
//
// CoverageUnknown has no Relay equivalent and no landed classification
// rule (B2's own addition). Treated the same as CoverageUnobserved, the
// identical narrow decision the Rust side makes (see conformance.rs's
// own doc comment): a device-reported indeterminate value is not
// authoritative evidence either way, so conformance cannot be claimed
// from it.
//
// Panics if coverage is not one of Coverage's four defined values --
// see Coverage.Valid's doc comment. The switch below enumerates
// CoverageUnobserved and CoverageUnknown explicitly rather than
// folding both into one default case, specifically so that case can
// never also catch a value Coverage.Valid would have rejected.
func ClassifyField(coverage Coverage, intentPresent, matched bool) FieldVerdict {
	if !coverage.Valid() {
		panic(fmt.Sprintf("conformance: invalid Coverage %q", string(coverage)))
	}
	switch coverage {
	case CoverageObserved:
		if matched {
			return VerdictConformant
		}
		return VerdictDrift
	case CoverageAbsent:
		if intentPresent {
			return VerdictDrift
		}
		return VerdictObservedAbsent
	case CoverageUnobserved, CoverageUnknown:
		return VerdictUnobserved
	default:
		// Unreachable: Valid() above already rejected anything else.
		panic(fmt.Sprintf("conformance: invalid Coverage %q", string(coverage)))
	}
}

// ClassifyFieldValue joins coverage with the value comparison -- the Go
// peer of conformance.rs's classify_field_value, identical to Relay's
// own Observation.ClassifyFieldValue. Only CoverageObserved ever reads
// a value; every other coverage state means the comparison is
// irrelevant and Compare is never invoked.
func ClassifyFieldValue(coverage Coverage, intentPresent bool, intent, observed interface{}, ct CompareType) FieldVerdict {
	if coverage != CoverageObserved {
		return ClassifyField(coverage, intentPresent, false)
	}
	matched, comparable := Compare(intent, observed, ct)
	if !comparable {
		return VerdictNotComparable
	}
	return ClassifyField(coverage, intentPresent, matched)
}
