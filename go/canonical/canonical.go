// Package canonical implements section A of docs/MATERIALIZATION-SPEC.md --
// one byte representation of a value tree, for digests and caches.
//
// This is the Go peer of this repo's own engine/src/canonical.rs, not a
// fresh re-derivation: both implement the identical, already-decided
// contract (section A), and both ultimately trace to the same source --
// CIC-Relay's pkg/canonicaljson/canonicaljson.go and
// core/nexus/iac/{number,digest}.go. Where the Rust side had to
// hand-port Go's behaviour (string escaping, float formatting, key
// sorting) because it had no access to Go's own standard library, this
// side simply reuses that standard library directly: encoding/json's
// string escaping and sort.Strings's byte-wise key order ARE section
// A4/A2, not separately-verified approximations of them.
//
// Unlike CIC-Relay's own split (a separate normalizeNumbers pass before
// canonicaljson.ToJSON), this writes a tree in one pass, mirroring
// canonical.rs's own to_canonical_json signature exactly -- an internal
// plumbing difference from the migration source, not a contract change.
//
// # What this does not cover yet
//
// The same two named, inherited gaps canonical.rs's own module doc
// names, neither fixed here either: Unicode normalization (A4) and
// TopologySet element canonical order (A6).
package canonical

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"math/big"
	"sort"
	"strconv"
)

// ToCanonicalJSON returns canonical JSON bytes for v (A1-A6): compact, no
// inter-token whitespace, object keys sorted by raw UTF-8 byte order,
// numbers per A3, strings escaped per A4, arrays kept in source order.
//
// Accepts the same value shapes encoding/json's own decoder produces:
// nil, bool, string, []interface{}, map[string]interface{}, any of Go's
// native integer/float kinds, and json.Number (this engine's equivalent
// of the Rust side's Value::BigInt, for integers wider than a machine
// word). A plain string is never treated as numeric, matching the Rust
// side's own Value::Str.
//
// Returns an error if any float leaf is NaN or infinite -- canonical
// JSON has no representation for either.
func ToCanonicalJSON(v interface{}) ([]byte, error) {
	var out bytes.Buffer
	if err := writeValue(&out, v, "$"); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}

// Digest returns "sha256:" + lowercase-hex(SHA-256(b)) -- A7. No
// transform beyond the hash itself; b is expected to already be
// canonical (normally the output of ToCanonicalJSON).
func Digest(b []byte) string {
	sum := sha256.Sum256(b)
	return "sha256:" + hex.EncodeToString(sum[:])
}

// IsValue reports whether v is a representable CIC value: nil, bool,
// string, any of Go's native integer/float kinds, json.Number, or a
// []interface{}/map[string]interface{} built recursively from the
// same set -- this package's own value domain, the Go peer of what
// the Rust side's Value enum can hold by construction (reader.rs).
//
// Review-caught on PR #32: unlike Rust, where an interface{}-typed
// argument has no equivalent (Value is a closed enum), Go's
// interface{} can hold ANY type -- a struct, a pointer, a []float64,
// a map[int]string -- none of which the Rust peer's closed value-tree
// can represent at all. A caller that accepts an interface{} value and
// treats its fmt.Sprintf("%v", ...) form as a stable identity or a
// canonical-byte input (go/collection's ElementKey; this package's own
// ToCanonicalJSON, which already rejects anything outside this domain
// via its own exhaustive type switch, independently of this function)
// must reject anything IsValue refuses first, not silently format or
// digest it -- the same "the type system can't close this off, so the
// boundary must" principle PR #31 already established for
// Coverage/CompareType, applied here to the value-tree shape itself,
// shared as one authority so collection/conformance need not each
// define their own copy of this domain.
//
// Deliberately does NOT reject NaN/Infinity -- shape and finiteness
// are orthogonal checks. Rust's own Value::Float can structurally hold
// a NaN too; it is ToCanonicalJSON's/canonical.rs's own finite check
// (and, in go/collection, its own non-finite pre-check) that rejects
// it, not the value type itself.
func IsValue(v interface{}) bool {
	switch vv := v.(type) {
	case nil, bool, string,
		int, int8, int16, int32, int64,
		uint, uint8, uint16, uint32, uint64,
		float32, float64, json.Number:
		return true
	case []interface{}:
		for _, item := range vv {
			if !IsValue(item) {
				return false
			}
		}
		return true
	case map[string]interface{}:
		for _, item := range vv {
			if !IsValue(item) {
				return false
			}
		}
		return true
	default:
		return false
	}
}

func writeValue(out *bytes.Buffer, v interface{}, path string) error {
	switch vv := v.(type) {
	case nil:
		out.WriteString("null")
	case bool:
		if vv {
			out.WriteString("true")
		} else {
			out.WriteString("false")
		}
	case string:
		writeJSONString(out, vv)
	case []interface{}:
		return writeArray(out, vv, path)
	case map[string]interface{}:
		return writeMap(out, vv, path)

	// Exact-digit rendering, no precision loss -- Go's own FormatInt/
	// FormatUint already guarantee this, the same guarantee i64's own
	// Display gives the Rust side for its Value::Int case.
	case int:
		out.WriteString(strconv.FormatInt(int64(vv), 10))
	case int8:
		out.WriteString(strconv.FormatInt(int64(vv), 10))
	case int16:
		out.WriteString(strconv.FormatInt(int64(vv), 10))
	case int32:
		out.WriteString(strconv.FormatInt(int64(vv), 10))
	case int64:
		out.WriteString(strconv.FormatInt(vv, 10))
	case uint:
		out.WriteString(strconv.FormatUint(uint64(vv), 10))
	case uint8:
		out.WriteString(strconv.FormatUint(uint64(vv), 10))
	case uint16:
		out.WriteString(strconv.FormatUint(uint64(vv), 10))
	case uint32:
		out.WriteString(strconv.FormatUint(uint64(vv), 10))
	case uint64:
		out.WriteString(strconv.FormatUint(vv, 10))

	case float32:
		return writeFloat(out, float64(vv), path)
	case float64:
		return writeFloat(out, vv, path)

	// This engine's big-integer-precision leaf (the Value::BigInt
	// peer): a json.Number may hold more digits than any machine
	// integer, so it is tried as an integer digit run first, full
	// precision, never routed through float64. Only if that fails
	// (json.Number also holds decimal/exponent literals, e.g. from a
	// json.Decoder with UseNumber() on "4.5" or "1e2") does it fall
	// back to the float path a float64 leaf would take.
	case json.Number:
		if s, ok := CanonicalInteger(string(vv)); ok {
			out.WriteString(s)
			return nil
		}
		f, err := strconv.ParseFloat(string(vv), 64)
		if err != nil {
			return fmt.Errorf("%s is %q, not a valid JSON number", path, string(vv))
		}
		return writeFloat(out, f, path)

	default:
		return fmt.Errorf("%s is a %T, not a representable canonical value", path, vv)
	}
	return nil
}

// writeFloat rejects NaN/infinity (canonical JSON has no representation
// for either) before writing CanonicalFloat's digits -- the Go peer of
// canonical.rs's own finite check on Value::Float.
func writeFloat(out *bytes.Buffer, f float64, path string) error {
	if math.IsNaN(f) || math.IsInf(f, 0) {
		return fmt.Errorf(
			"%s is %v, not a finite number; canonical JSON has no representation for NaN or infinity",
			path, f,
		)
	}
	out.WriteString(CanonicalFloat(f))
	return nil
}

func writeArray(out *bytes.Buffer, items []interface{}, path string) error {
	out.WriteByte('[')
	for i, item := range items {
		if i > 0 {
			out.WriteByte(',')
		}
		if err := writeValue(out, item, fmt.Sprintf("%s[%d]", path, i)); err != nil {
			return err
		}
	}
	out.WriteByte(']')
	return nil
}

func writeMap(out *bytes.Buffer, m map[string]interface{}, path string) error {
	out.WriteByte('{')
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	// Raw UTF-8 byte order (A2) -- sort.Strings is already byte-wise
	// comparison on Go's UTF-8-encoded strings, no custom comparator
	// needed, matching Rust's str::Ord the Rust side relies on for the
	// identical reason.
	sort.Strings(keys)
	for i, k := range keys {
		if i > 0 {
			out.WriteByte(',')
		}
		writeJSONString(out, k)
		out.WriteByte(':')
		if err := writeValue(out, m[k], path+"."+k); err != nil {
			return err
		}
	}
	out.WriteByte('}')
	return nil
}

// writeJSONString writes v as a JSON string token using encoding/json's
// own marshaler (A4) -- this IS section A4's escaping rule, not an
// approximation of it: the short escapes, the \uXXXX form for every
// other control character below U+0020, and the same \uXXXX form for
// '<', '>', '&', U+2028 and U+2029 (HTML-safe escaping, encoding/json's
// default, inherited rather than a deliberate design choice -- see
// canonical.rs's own doc comment for why changing it would silently
// change every existing digest). No Unicode normalization is applied.
func writeJSONString(out *bytes.Buffer, v string) {
	b, _ := json.Marshal(v) // string marshaling to JSON never errors
	out.Write(b)
}

// CanonicalFloat renders f in plain (non-exponent) decimal, shortest
// round-trip form, mapping negative zero to "0" -- A3, identical to
// CIC-Relay's own core/nexus/iac/number.go canonicalFloat and to this
// repo's Rust canonical_float (engine/src/canonical.rs), all three
// built on the same strconv.FormatFloat(f, 'f', -1, 64) guarantee.
func CanonicalFloat(f float64) string {
	s := strconv.FormatFloat(f, 'f', -1, 64)
	if s == "-0" {
		return "0"
	}
	return s
}

// CanonicalInteger normalizes a signed decimal digit run -- A3's
// big.Int-equivalent case, this repo's Rust canonical_integer's peer:
// sign folded, leading zeros stripped, "-0"/"+0" -> "0". Returns
// ok=false if s is not actually a plain signed digit run (unlike the
// Rust side, which can assume this precondition from Value::BigInt's
// own invariant, this package has no such type to lean on -- json.Number
// is not guaranteed to hold only digits, since encoding/json also uses
// it for decimal/exponent literals -- so this function checks rather
// than assumes).
func CanonicalInteger(s string) (string, bool) {
	i, ok := new(big.Int).SetString(s, 10)
	if !ok {
		return "", false
	}
	return i.String(), true
}
