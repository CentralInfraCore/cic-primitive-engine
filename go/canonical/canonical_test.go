package canonical

import (
	"encoding/json"
	"fmt"
	"math"
	"testing"
)

func mustJSON(t *testing.T, v interface{}) string {
	t.Helper()
	b, err := ToCanonicalJSON(v)
	if err != nil {
		t.Fatalf("ToCanonicalJSON(%#v): %v", v, err)
	}
	return string(b)
}

// Mirrors engine/src/canonical.rs's data_types_mirror_go, which itself
// mirrors TestToJSON_DataTypes in pkg/canonicaljson/canonicaljson_test.go.
func TestDataTypesMirrorRust(t *testing.T) {
	cases := []struct {
		name string
		v    interface{}
		want string
	}{
		{"null", nil, "null"},
		{"true", true, "true"},
		{"false", false, "false"},
		{"float", 123.456, "123.456"},
		{"zero float", 0.0, "0"},
		{"string", "hello world", "\"hello world\""},
		{"empty array", []interface{}{}, "[]"},
		{
			"mixed array",
			[]interface{}{1.0, "two", true, nil},
			"[1,\"two\",true,null]",
		},
		{"empty map", map[string]interface{}{}, "{}"},
		{
			"sorted map",
			map[string]interface{}{"c": 3.0, "a": 1.0, "b": 2.0},
			"{\"a\":1,\"b\":2,\"c\":3}",
		},
		{"int", 42, "42"},
	}
	for _, c := range cases {
		if got := mustJSON(t, c.v); got != c.want {
			t.Errorf("%s: got %q, want %q", c.name, got, c.want)
		}
	}
}

// Mirrors canonical.rs's nested_structure_mirrors_go.
func TestNestedStructureMirrorsRust(t *testing.T) {
	nested := map[string]interface{}{
		"zulu": "last",
		"alpha": []interface{}{
			"one",
			map[string]interface{}{"gamma": true, "beta": false},
		},
		"x-ray": 123,
	}
	want := `{"alpha":["one",{"beta":false,"gamma":true}],"x-ray":123,"zulu":"last"}`
	if got := mustJSON(t, nested); got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

// Mirrors canonical.rs's numbers_mirror_cic_canonical.
func TestNumbersMirrorRust(t *testing.T) {
	cases := []struct {
		v    interface{}
		want string
	}{
		{4.0, "4"},
		{4.5, "4.5"},
		{-0.0, "0"},
		{1000.0, "1000"},
		{-7, "-7"},
		{0, "0"},
		// Beyond f64's 53-bit mantissa: int64 keeps full precision, no
		// float64 round-trip -- the same boundary engine/src/canonical.rs
		// pins with Value::Int(i64).
		{int64(9_007_199_254_740_993), "9007199254740993"},
		{int64(9_007_199_254_740_992), "9007199254740992"},
	}
	for _, c := range cases {
		if got := mustJSON(t, c.v); got != c.want {
			t.Errorf("%v: got %q, want %q", c.v, got, c.want)
		}
	}
}

// Mirrors canonical.rs's extreme_magnitude_floats_match_go_format_float_exactly.
func TestExtremeMagnitudeFloats(t *testing.T) {
	cases := []struct {
		v    float64
		want string
	}{
		{1e20, "100000000000000000000"},
		{1e21, "1000000000000000000000"},
		{1e30, "1000000000000000000000000000000"},
		{1e-10, "0.0000000001"},
		{1e-100, "0.0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001"},
		{5e-324, "0.000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000005"},
	}
	for _, c := range cases {
		if got := mustJSON(t, c.v); got != c.want {
			t.Errorf("%v: got %q, want %q", c.v, got, c.want)
		}
	}
}

// Mirrors canonical.rs's non_finite_floats_are_rejected_not_written.
func TestNonFiniteFloatsAreRejected(t *testing.T) {
	for _, v := range []float64{math.NaN(), math.Inf(1), math.Inf(-1)} {
		if _, err := ToCanonicalJSON(v); err == nil {
			t.Errorf("%v: expected rejection, got none", v)
		}
	}
	// The rejection carries the path to the offending value, not just
	// "somewhere in the tree".
	nested := map[string]interface{}{"a": []interface{}{1, math.NaN()}}
	_, err := ToCanonicalJSON(nested)
	if err == nil {
		t.Fatal("expected rejection for nested NaN")
	}
	if want := "$.a[1]"; !contains(err.Error(), want) {
		t.Errorf("error %q does not name path %q", err.Error(), want)
	}
}

func contains(s, substr string) bool {
	return len(s) >= len(substr) && indexOf(s, substr) >= 0
}

func indexOf(s, substr string) int {
	for i := 0; i+len(substr) <= len(s); i++ {
		if s[i:i+len(substr)] == substr {
			return i
		}
	}
	return -1
}

// Mirrors canonical.rs's string_escaping_matches_go_json_marshal.
func TestStringEscaping(t *testing.T) {
	escLt := fmt.Sprintf("\\u%04x", '<')
	escGt := fmt.Sprintf("\\u%04x", '>')
	escAmp := fmt.Sprintf("\\u%04x", '&')
	want := "\"a" + escLt + "b" + escGt + "c" + escAmp + "d\""
	if got := mustJSON(t, "a<b>c&d"); got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	lineSep := string(rune(0x2028))
	paraSep := string(rune(0x2029))
	escLine := fmt.Sprintf("\\u%04x", 0x2028)
	escPara := fmt.Sprintf("\\u%04x", 0x2029)
	want2 := "\"line" + escLine + "sep" + escPara + "para\""
	if got := mustJSON(t, "line"+lineSep+"sep"+paraSep+"para"); got != want2 {
		t.Errorf("got %q, want %q", got, want2)
	}
	// Precomposed "café" (U+00E9) -- different UTF-8 bytes from the
	// decomposed form below, by design (A4's named Unicode-normalization
	// gap): both are legal, neither is canonicalized toward the other.
	b, err := ToCanonicalJSON("café")
	if err != nil {
		t.Fatal(err)
	}
	if want := "\"caf\xc3\xa9\""; string(b) != want {
		t.Errorf("got %q, want %q", b, want)
	}
	b, err = ToCanonicalJSON("café")
	if err != nil {
		t.Fatal(err)
	}
	if want := "\"cafe\xcc\x81\""; string(b) != want {
		t.Errorf("got %q, want %q", b, want)
	}
}

// Mirrors canonical.rs's control_character_escaping_matches_go_json_marshal.
func TestControlCharacterEscaping(t *testing.T) {
	cases := []struct {
		ch      rune
		escaped string
	}{
		{0x00, `\u0000`},
		{0x07, `\u0007`},
		{0x08, `\b`},
		{0x09, `\t`},
		{0x0A, `\n`},
		{0x0B, `\u000b`},
		{0x0C, `\f`},
		{0x0D, `\r`},
		{0x0E, `\u000e`},
		{0x1F, `\u001f`},
	}
	for _, c := range cases {
		want := "\"" + c.escaped + "\""
		if got := mustJSON(t, string(c.ch)); got != want {
			t.Errorf("char %U: got %q, want %q", c.ch, got, want)
		}
	}
	// DEL (0x7F) is NOT escaped by encoding/json.Marshal, and must not
	// be escaped here either.
	b, err := ToCanonicalJSON(string(rune(0x7F)))
	if err != nil {
		t.Fatal(err)
	}
	if want := []byte{'"', 0x7F, '"'}; string(b) != string(want) {
		t.Errorf("got %v, want %v", b, want)
	}
}

func TestDigestIsSHA256WithLowercaseHexPrefix(t *testing.T) {
	d := Digest([]byte("{}"))
	if len(d) != len("sha256:")+64 {
		t.Fatalf("unexpected digest length: %q", d)
	}
	if d[:len("sha256:")] != "sha256:" {
		t.Fatalf("missing sha256: prefix: %q", d)
	}
	if Digest([]byte("{}")) != d {
		t.Error("digest is not deterministic")
	}
	if Digest([]byte("[]")) == d {
		t.Error("digest is not sensitive to its input")
	}
}

// Mirrors canonical.rs's big_integers_normalize_like_go_big_int, using
// json.Number as the engine's Value::BigInt peer.
func TestBigIntegersNormalize(t *testing.T) {
	cases := []struct {
		literal string
		want    string
	}{
		{"9223372036854775808", "9223372036854775808"},
		{"9223372036854775809", "9223372036854775809"},
		{"-9223372036854775809", "-9223372036854775809"},
		{"000000123", "123"},
		{"-00000123", "-123"},
		{"0", "0"},
		{"-0", "0"},
		{"+0", "0"},
		{"+123", "123"},
		{
			"1234567890123456789012345678901234567890",
			"1234567890123456789012345678901234567890",
		},
		{
			"-1234567890123456789012345678901234567890",
			"-1234567890123456789012345678901234567890",
		},
	}
	for _, c := range cases {
		if got := mustJSON(t, json.Number(c.literal)); got != c.want {
			t.Errorf("literal %q: got %q, want %q", c.literal, got, c.want)
		}
	}
}

type testStruct struct{ X int }

// IsValue was added on PR #32, for go/collection's ElementKey to
// validate its input against -- see that package for the bug this
// closed. Verified directly here too: every shape this package's own
// ToCanonicalJSON accepts must be IsValue, and the shapes a Go
// interface{} can hold that have no Rust Value equivalent (a struct, a
// pointer, a non-string-keyed map, a non-interface{}-element slice)
// must not be.
func TestIsValueAcceptsExactlyTheSupportedShapes(t *testing.T) {
	valid := []interface{}{
		nil, true, "s", 1, int64(1), float64(1.5), json.Number("1"),
		[]interface{}{}, []interface{}{1, "s", nil},
		map[string]interface{}{}, map[string]interface{}{"a": 1},
		// Nested, including a non-finite leaf -- IsValue is a shape
		// check only; finiteness is a separate, orthogonal concern
		// (see this function's own doc comment).
		[]interface{}{math.NaN()},
		map[string]interface{}{"nested": []interface{}{1, 2}},
	}
	for _, v := range valid {
		if !IsValue(v) {
			t.Errorf("IsValue(%#v) = false, want true", v)
		}
	}

	invalid := []interface{}{
		testStruct{1},
		&testStruct{1},
		[]float64{1, 2},
		map[int]string{1: "x"},
		[]interface{}{testStruct{1}},                 // invalid nested in a valid slice
		map[string]interface{}{"a": testStruct{1}},   // invalid nested in a valid map
		map[string]interface{}{"a": []float64{1, 2}}, // wrong slice element type, nested
	}
	for _, v := range invalid {
		if IsValue(v) {
			t.Errorf("IsValue(%#v) = true, want false", v)
		}
	}
}
