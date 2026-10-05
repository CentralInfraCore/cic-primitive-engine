package collection

import (
	"encoding/json"
	"math"
	"testing"
)

// Mirrors engine/src/collection.rs's element_key_mirrors_go, which
// itself mirrors TestCollection_ElementKey in collection_test.go, case
// for case, against the real Go vectors.
func TestElementKey(t *testing.T) {
	mapColl := Collection{Topology: TopologyMap, Keys: []string{"name"}}
	if got, want := mapColl.ElementKey(map[string]interface{}{
		"name": "nic-0", "subnet": "a",
	}), "name=nic-0"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	// multi-key: stable, sorted regardless of declared order.
	multi := Collection{Topology: TopologyMap, Keys: []string{"zone", "name"}}
	if got, want := multi.ElementKey(map[string]interface{}{
		"name": "nic-0", "zone": "eu",
	}), "name=nic-0,zone=eu"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	// map topology, non-map element -> no key.
	if got := mapColl.ElementKey("scalar"); got != "" {
		t.Errorf("got %q, want empty", got)
	}

	// missing key field rendered explicitly, not omitted -- for free,
	// via Go's own map-indexing + fmt.Sprintf("%v", nil); see the
	// package doc.
	if got, want := mapColl.ElementKey(map[string]interface{}{"subnet": "a"}), "name=<nil>"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	// set -> value form.
	setColl := Collection{Topology: TopologySet}
	if got, want := setColl.ElementKey("10.0.0.0/16"), "10.0.0.0/16"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	// atomic / unset -> no per-element identity.
	if got := (Collection{Topology: TopologyAtomic}).ElementKey("x"); got != "" {
		t.Errorf("got %q, want empty", got)
	}
}

// Mirrors collection.rs's float_values_get_the_same_identity_go_does.
// These vectors are a differential pin against the Rust side's own
// table (both were originally generated from real Go fmt.Sprintf("%v",
// ...) output), not new coverage of anything this package itself
// implements -- it just calls fmt.Sprintf directly.
func TestFloatValuesGetTheSameIdentityAsRust(t *testing.T) {
	set := Collection{Topology: TopologySet}
	cases := []struct {
		f    float64
		want string
	}{
		{16.0, "16"},
		{16.5, "16.5"},
		{0.1, "0.1"},
		{0.0001, "0.0001"},
		{0.00001, "1e-05"},
		{100000.0, "100000"},
		{1_000_000.0, "1e+06"},
		{123456.0, "123456"},
		{1_234_567.0, "1.234567e+06"},
		{1e20, "1e+20"},
		{1e-100, "1e-100"},
		{-16.5, "-16.5"},
		{0.0, "0"},
	}
	for _, c := range cases {
		if got := set.ElementKey(c.f); got != c.want {
			t.Errorf("%v: got %q, want %q", c.f, got, c.want)
		}
	}

	mapColl := Collection{Topology: TopologyMap, Keys: []string{"id"}}
	if got, want := mapColl.ElementKey(map[string]interface{}{"id": 1.5}), "id=1.5"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	// NaN/Infinity: the one residual, defensively-handled exception.
	if got := set.ElementKey(math.NaN()); got != "" {
		t.Errorf("NaN: got %q, want empty", got)
	}
	if got := set.ElementKey(math.Inf(1)); got != "" {
		t.Errorf("+Inf: got %q, want empty", got)
	}
}

// Found while writing this package, not carried over from the Rust
// side: fmt.Sprintf("%v", ...) does NOT fold negative zero, unlike
// section A's own canonical_float (a different algorithm, not used
// here). A genuine runtime negative zero -- e.g. from
// json.Unmarshal([]byte("-0.0"), &f), or from math.Copysign -- prints
// "-0", not "0". This caught a real bug on the Rust peer
// (engine/src/collection.rs's own go_float_display used to fold both
// signs to "0" unconditionally; fixed alongside this package, PR #32).
//
// Note the asymmetry with a Go SOURCE literal: unlike Rust, where
// -0.0 is a genuine sign-preserving negative zero even as a literal,
// Go's untyped float constant arithmetic folds a literal -0.0 to
// ordinary +0.0 at compile time -- fmt.Sprintf("%v", -0.0) in Go
// source prints "0". Only a runtime-computed or decoded negative zero
// keeps its sign, which is also the realistic case (an authored
// "-0.0" in actual input), so that's what this test exercises.
func TestNegativeZeroKeepsItsSign(t *testing.T) {
	set := Collection{Topology: TopologySet}
	if got := set.ElementKey(0.0); got != "0" {
		t.Errorf("+0: got %q, want %q", got, "0")
	}
	if got := set.ElementKey(math.Copysign(0, -1)); got != "-0" {
		t.Errorf("-0 (Copysign): got %q, want %q", got, "-0")
	}

	var decoded float64
	if err := json.Unmarshal([]byte("-0.0"), &decoded); err != nil {
		t.Fatal(err)
	}
	if !math.Signbit(decoded) {
		t.Fatal("decoded -0.0 should have its sign bit set")
	}
	if got := set.ElementKey(decoded); got != "-0" {
		t.Errorf("-0 (decoded): got %q, want %q", got, "-0")
	}
}

// Mirrors collection.rs's nested_seq_and_map_values_match_go_v_formatting.
func TestNestedValuesMatchGoVFormatting(t *testing.T) {
	set := Collection{Topology: TopologySet}
	if got, want := set.ElementKey([]interface{}{"a", 1, true}), "[a 1 true]"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
	// Go sorts map keys for %v regardless of insertion order.
	if got, want := set.ElementKey(map[string]interface{}{"b": 1, "a": "x"}), "map[a:x b:1]"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
	if got := set.ElementKey([]interface{}{}); got != "[]" {
		t.Errorf("got %q, want [[]]", got)
	}
	if got := set.ElementKey(map[string]interface{}{}); got != "map[]" {
		t.Errorf("got %q, want map[]", got)
	}
	// A non-finite float nested inside a Seq/Map must still be refused,
	// not silently printed as "NaN" -- containsNonFinite must recurse,
	// not just check the top-level value.
	if got := set.ElementKey([]interface{}{1, math.NaN()}); got != "" {
		t.Errorf("nested NaN in slice: got %q, want empty", got)
	}
	if got := set.ElementKey(map[string]interface{}{"a": math.Inf(-1)}); got != "" {
		t.Errorf("nested -Inf in map: got %q, want empty", got)
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

// Applying PR #31's review lesson proactively here: CollectionTopology
// is a string-backed Go type, so CollectionTopology("garbage") compiles
// and type-checks even though the equivalent would not build on the
// Rust side. ElementKey must reject it, not silently treat it as
// TopologyAtomic (which a bare "default: return \"\"" without a prior
// Valid() check would do).
func TestInvalidTopologyFailsClosed(t *testing.T) {
	expectPanic(t, "ElementKey", func() {
		Collection{Topology: CollectionTopology("garbage")}.ElementKey("x")
	})
}

func TestTopologyValidReportsExactlyTheDefinedConstants(t *testing.T) {
	for _, topo := range []CollectionTopology{TopologyAtomic, TopologySet, TopologyMap} {
		if !topo.Valid() {
			t.Errorf("topology %q should be valid", topo)
		}
	}
	if CollectionTopology("garbage").Valid() {
		t.Error("CollectionTopology(\"garbage\") should not be valid")
	}
}

// The exact string literals F5's PlanDigestProjection commits to per
// collection -- verified directly against collection.go's own three
// CollectionTopology constants.
func TestTopologyStringsMatchSpec(t *testing.T) {
	cases := []struct{ got, want string }{
		{string(TopologyAtomic), "atomic"},
		{string(TopologySet), "set"},
		{string(TopologyMap), "map"},
	}
	for _, c := range cases {
		if c.got != c.want {
			t.Errorf("got %q, want %q", c.got, c.want)
		}
	}
}
