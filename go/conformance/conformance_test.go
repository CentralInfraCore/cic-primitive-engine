package conformance

import (
	"math/big"
	"testing"
)

// Mirrors engine/src/conformance.rs's compare_exact_mirrors_go, which
// itself mirrors TestCompare_Exact in compare_test.go, case for case.
func TestCompareExact(t *testing.T) {
	cases := []struct {
		name           string
		intent         interface{}
		observed       interface{}
		wantMatched    bool
		wantComparable bool
	}{
		{"equal strings", "prod", "prod", true, true},
		{"different strings", "prod", "dev", false, true},
		{"equal ints", 16, 16, true, true},
		{"different ints", 16, 32, false, true},
		{"int vs float same value", 16, 16.0, true, true},
		{"string vs number -> not comparable", "16", 16, false, false},
		{"null vs string -> not comparable", nil, "x", false, false},
		{"equal nulls", nil, nil, true, true},
	}
	for _, c := range cases {
		matched, comparable := Compare(c.intent, c.observed, CompareExact)
		if comparable != c.wantComparable {
			t.Errorf("%s: comparable = %v, want %v", c.name, comparable, c.wantComparable)
			continue
		}
		if c.wantComparable && matched != c.wantMatched {
			t.Errorf("%s: matched = %v, want %v", c.name, matched, c.wantMatched)
		}
	}

	// "number vs object -> not comparable"
	if _, comparable := Compare(16, map[string]interface{}{}, CompareExact); comparable {
		t.Error("number vs object must not be comparable")
	}
	// "equal maps" / "different maps"
	a := map[string]interface{}{"a": "x"}
	b := map[string]interface{}{"a": "x"}
	c := map[string]interface{}{"a": "y"}
	if matched, comparable := Compare(a, b, CompareExact); !comparable || !matched {
		t.Error("equal maps must match")
	}
	if matched, comparable := Compare(a, c, CompareExact); !comparable || matched {
		t.Error("different maps must not match")
	}
}

// Mirrors conformance.rs's compare_numeric_mirrors_go, which itself
// mirrors TestCompare_Numeric in compare_test.go, case for case.
func TestCompareNumeric(t *testing.T) {
	cases := []struct {
		name           string
		intent         interface{}
		observed       interface{}
		wantMatched    bool
		wantComparable bool
	}{
		{"int vs float same value", 1, 1.0, true, true},
		{"int vs numeric string", 1, "1", true, true},
		{"float vs scientific string", 100.0, "1e2", true, true},
		{"decimal string equality", "1.5", 1.5, true, true},
		{"different numbers", 1, 2, false, true},
		{"non-numeric string -> not comparable", "abc", 1, false, false},
		{"bool is not numeric", true, 1, false, false},
	}
	for _, c := range cases {
		matched, comparable := Compare(c.intent, c.observed, CompareNumeric)
		if comparable != c.wantComparable {
			t.Errorf("%s: comparable = %v, want %v", c.name, comparable, c.wantComparable)
			continue
		}
		if c.wantComparable && matched != c.wantMatched {
			t.Errorf("%s: matched = %v, want %v", c.name, matched, c.wantMatched)
		}
	}
	if _, comparable := Compare(map[string]interface{}{}, 1, CompareNumeric); comparable {
		t.Error("object -> not comparable under numeric")
	}
}

// The precision fact conformance.rs's own doc comment names: "0.1"
// parses EXACTLY as 1/10, not the binary-rounded float64 value. Here
// this is simply big.Rat.SetString's own documented behaviour -- no
// hand-written grammar to verify, see the package doc for why.
func TestDecimalStringParsesExactNotBinaryRounded(t *testing.T) {
	exact, ok := new(big.Rat).SetString("0.1")
	if !ok {
		t.Fatal("\"0.1\" should parse")
	}
	fromBinaryFloat := new(big.Rat).SetFloat64(0.1)
	if exact.Cmp(fromBinaryFloat) == 0 {
		t.Error("0.1 the decimal string must NOT equal 0.1 the binary float -- this is the real asymmetry, not a bug")
	}
	if exact.Num().Cmp(big.NewInt(1)) != 0 || exact.Denom().Cmp(big.NewInt(10)) != 0 {
		t.Errorf("got %v/%v, want 1/10", exact.Num(), exact.Denom())
	}
}

// Representative subset of conformance.rs's based_integers_floats_and_
// underscores_match_go_big_rat vectors (PR #25's grammar-parity gap,
// closed there by hand-porting big.Rat.SetString's grammar). Here
// they exercise the real function those vectors were written to
// match, not a second implementation of it -- kept as a differential
// pin against the Rust side's own table, not as new coverage.
func TestNumericComparatorHandlesSetStringGrammar(t *testing.T) {
	cases := []struct {
		name        string
		intent      interface{}
		observed    interface{}
		wantMatched bool
	}{
		{"fraction vs decimal", "1/2", "0.5", true},
		{"hex vs decimal", "0x10", "16", true},
		{"octal vs decimal", "0o17", "15", true},
		{"binary vs decimal", "0b101", "5", true},
		{"underscore digit separator", "1_000", "1000", true},
		{"hex float with p exponent", "0x1.8p1", "3", true},
	}
	for _, c := range cases {
		matched, comparable := Compare(c.intent, c.observed, CompareNumeric)
		if !comparable {
			t.Errorf("%s: expected comparable", c.name)
			continue
		}
		if matched != c.wantMatched {
			t.Errorf("%s: matched = %v, want %v", c.name, matched, c.wantMatched)
		}
	}
}

// Mirrors conformance.rs's classify_field_value_mirrors_go, which
// itself mirrors TestClassifyFieldValue in compare_test.go, case for
// case -- using Coverage directly rather than Relay's
// Observation{Observed, AuthoritativeAbsent} envelope, since this
// package has no object-level walker (and no two-list envelope)
// either; see the package doc.
func TestClassifyFieldValue(t *testing.T) {
	flex4, flex3 := "E4.Flex", "E3.Flex"

	// observed + matched -> CONFORMANT
	if got := ClassifyFieldValue(CoverageObserved, true, flex4, flex4, CompareExact); got != VerdictConformant {
		t.Errorf("observed+matched: got %v, want CONFORMANT", got)
	}
	// observed + mismatch -> DRIFT
	if got := ClassifyFieldValue(CoverageObserved, true, flex4, flex3, CompareExact); got != VerdictDrift {
		t.Errorf("observed+mismatch: got %v, want DRIFT", got)
	}
	// observed + not comparable -> NOT_COMPARABLE
	if got := ClassifyFieldValue(CoverageObserved, true, "16", 16, CompareExact); got != VerdictNotComparable {
		t.Errorf("not comparable: got %v, want NOT_COMPARABLE", got)
	}
	// numeric normalization -> CONFORMANT
	if got := ClassifyFieldValue(CoverageObserved, true, 16, 16.0, CompareNumeric); got != VerdictConformant {
		t.Errorf("numeric normalization: got %v, want CONFORMANT", got)
	}
	// unobserved -> UNOBSERVED (value ignored)
	if got := ClassifyFieldValue(CoverageUnobserved, true, flex4, "anything", CompareExact); got != VerdictUnobserved {
		t.Errorf("unobserved: got %v, want UNOBSERVED", got)
	}
	// authoritative absent + intent wants it -> DRIFT
	if got := ClassifyFieldValue(CoverageAbsent, true, nil, nil, CompareExact); got != VerdictDrift {
		t.Errorf("absent+intent wants it: got %v, want DRIFT", got)
	}
	// authoritative absent + intent omits it -> OBSERVED_ABSENT
	if got := ClassifyFieldValue(CoverageAbsent, false, nil, nil, CompareExact); got != VerdictObservedAbsent {
		t.Errorf("absent+intent omits it: got %v, want OBSERVED_ABSENT", got)
	}
}

// No Relay precedent for this -- CoverageUnknown is B2's own addition.
// Pinned down here as a decision this package makes, not inherited,
// mirroring conformance.rs's unknown_coverage_classifies_like_unobserved.
func TestUnknownCoverageClassifiesLikeUnobserved(t *testing.T) {
	if got := ClassifyFieldValue(CoverageUnknown, true, "x", "fault", CompareExact); got != VerdictUnobserved {
		t.Errorf("got %v, want UNOBSERVED", got)
	}
}

func TestObservationDefaultsToUnobserved(t *testing.T) {
	o := NewObservation()
	if got := o.Coverage("/never/set"); got != CoverageUnobserved {
		t.Errorf("got %v, want unobserved", got)
	}
	o.Set("/memory_gb", CoverageObserved)
	if got := o.Coverage("/memory_gb"); got != CoverageObserved {
		t.Errorf("got %v, want observed", got)
	}
}

// The exact string literals docs/VERDICT-SCHEMA.md's fields map and
// F5's ObservationDigestProjection commit to per path -- verified
// directly against observation.go's own constants.
func TestCoverageAndVerdictStringsMatchSpec(t *testing.T) {
	cases := []struct {
		got  string
		want string
	}{
		{string(CoverageObserved), "observed"},
		{string(CoverageAbsent), "absent"},
		{string(CoverageUnobserved), "unobserved"},
		{string(CoverageUnknown), "unknown"},
		{string(CompareExact), "exact"},
		{string(CompareNumeric), "numeric"},
		{string(VerdictConformant), "CONFORMANT"},
		{string(VerdictDrift), "DRIFT"},
		{string(VerdictObservedAbsent), "OBSERVED_ABSENT"},
		{string(VerdictUnobserved), "UNOBSERVED"},
		{string(VerdictNotComparable), "NOT_COMPARABLE"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("got %q, want %q", c.got, c.want)
		}
	}
}
