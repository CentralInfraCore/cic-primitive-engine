// Package collection implements collection topology -- how a list
// field's elements are identified (docs/MATERIALIZATION-SPEC.md,
// section F) -- the Go peer of this repo's own engine/src/collection.rs
// (F7). Both ultimately trace to the same source, CIC-Relay's
// core/nexus/iac/collection.go, taken there (deliberately, not
// reinvented) from Kubernetes server-side-apply: a list is atomic, a
// set, or a map keyed by named fields.
//
// # ElementKey's identity formatting IS Go's own fmt.Sprintf("%v", ...)
//
// engine/src/collection.rs had to hand-port fmt.Sprintf("%v", ...)'s
// exact behaviour for every value shape -- float formatting
// (strconv.FormatFloat(f, 'g', -1, 64)'s plain-vs-scientific
// threshold), slice/map rendering ("[e1 e2]" / "map[k1:v1 k2:v2]", map
// keys sorted since Go 1.12) -- because Rust has no access to Go's own
// fmt package. This package has nothing to hand-port here: it calls
// fmt.Sprintf("%v", v) directly on the real value, exactly as
// collection.go's own ElementKey already does. The one piece written
// by hand is containsNonFinite, a recursive pre-check rejecting any
// NaN/Infinity reachable in v -- %v would happily print "NaN"/"+Inf" as
// if it were a stable identity, which engine/src/collection.rs's own
// module doc already treats as a defensive, documented exception
// rather than a real identity (a materialized value in either
// language's own pipeline never carries one, since section A's
// Stage::Canonicalize already rejects non-finite floats).
//
// # A missing map-topology key field renders "<nil>" for free
//
// engine/src/collection.rs has to special-case a missing key field
// ("a missing key field renders as `<nil>`, matching Go's
// fmt.Sprintf(\"%v\", nil) exactly") because Rust's Value::Map has no
// notion of "absent", only "present". This package needs no such
// special case: indexing a Go map[string]interface{} with a missing
// key already returns the nil interface{} zero value, and
// fmt.Sprintf("%v", nil) already prints "<nil>" -- both for free, not
// written here.
//
// # ElementKey's input domain is validated, not assumed
//
// Review-caught on PR #32: calling fmt.Sprintf("%v", elem) directly,
// as the first version of this file did, happily formats ANY Go value
// -- a struct, a pointer, a []float64, a map[int]string -- into a
// plausible-looking identity with no equivalent in the Rust peer's
// closed Value-tree (collection.rs's elem is a &Value, which simply
// cannot be any of those shapes). Worse, the non-finite pre-check only
// ever recursed into []interface{}/map[string]interface{}, so a
// []float64{1, NaN} slipped past it entirely and produced the exact
// "NaN masquerading as a stable identity" result the pre-check exists
// to prevent, just via a container shape it didn't recognize. Fixed:
// ElementKey validates elem (and, for map topology, the whole element,
// not only the key fields it reads) against canonical.IsValue first,
// and panics on anything outside that domain -- the same
// "the type system can't close this off, so the boundary must"
// principle PR #31 already established for Coverage/CompareType,
// shared from canonical rather than redefined here.
package collection

import (
	"fmt"
	"math"
	"sort"
	"strings"

	"github.com/CentralInfraCore/cic-primitive-engine/go/canonical"
)

// CollectionTopology names how a list field's elements are identified.
type CollectionTopology string

const (
	// TopologyAtomic -- the whole list is a single opaque value (one owner).
	TopologyAtomic CollectionTopology = "atomic"
	// TopologySet -- elements are unique by value; order is irrelevant.
	TopologySet CollectionTopology = "set"
	// TopologyMap -- elements are identified by one or more key fields.
	TopologyMap CollectionTopology = "map"
)

// Valid reports whether t is one of the three topologies this package
// defines. See package conformance's Coverage.Valid (go/conformance)
// for why Collection.ElementKey checks this and panics on an invalid
// value rather than silently falling through to a plausible-looking
// default (PR #31's review caught exactly that shape of bug for
// Coverage/CompareType; applied here from the start rather than
// waiting for the same gap to be found a third time).
func (t CollectionTopology) Valid() bool {
	switch t {
	case TopologyAtomic, TopologySet, TopologyMap:
		return true
	}
	return false
}

// Collection describes a list field's topology -- the Go peer of
// collection.rs's own Collection struct. Keys applies only to
// TopologyMap.
type Collection struct {
	Topology CollectionTopology
	Keys     []string
}

// ElementKey returns a stable identity for a list element under this
// topology -- the Go peer of collection.rs's own element_key, identical
// to collection.go's own ElementKey:
//
//   - TopologyMap: the sorted "k=v" tuple of the element's key fields
//     ("name=nic-0"); a missing key field renders as "<nil>" (see the
//     package doc for why this needs no special case here), and elem
//     not being a map[string]interface{} at all yields "" -- map
//     topology, non-map element, no key.
//   - TopologySet: fmt.Sprintf("%v", elem) in full, nested structures
//     included -- Go's own reference still calls this "a placeholder
//     identity until the CIC Canonical Object Encoding lands," a
//     property of the thing being ported, not of how faithfully it's
//     ported here.
//   - TopologyAtomic: "" -- the list has no per-element identity, it
//     is one value.
//
// Panics if c.Topology is not one of CollectionTopology's three
// defined values -- see CollectionTopology.Valid's doc comment.
func (c Collection) ElementKey(elem interface{}) string {
	if !c.Topology.Valid() {
		panic(fmt.Sprintf("collection: invalid CollectionTopology %q", string(c.Topology)))
	}
	switch c.Topology {
	case TopologyMap:
		if !canonical.IsValue(elem) {
			panic(fmt.Sprintf("collection: elem is not a representable CIC value (%T)", elem))
		}
		m, ok := elem.(map[string]interface{})
		if !ok {
			return "" // map topology, non-map element -> no key
		}
		keys := append([]string(nil), c.Keys...)
		sort.Strings(keys) // stable regardless of declared order
		parts := make([]string, 0, len(keys))
		for _, k := range keys {
			display, ok := goDisplay(m[k])
			if !ok {
				return "" // only a non-finite float reaches here -- no identity, not guessed
			}
			parts = append(parts, k+"="+display)
		}
		return strings.Join(parts, ",")
	case TopologySet:
		if !canonical.IsValue(elem) {
			panic(fmt.Sprintf("collection: elem is not a representable CIC value (%T)", elem))
		}
		display, ok := goDisplay(elem)
		if !ok {
			return ""
		}
		return display
	default: // TopologyAtomic (Valid() above already rejected anything else)
		// elem is never inspected for atomic topology -- there is no
		// per-element identity to format, so there is nothing to
		// validate either, matching collection.rs's own element_key,
		// which never looks at its argument in this branch.
		return ""
	}
}

// goDisplay is fmt.Sprintf("%v", v), refused (ok=false) if v is, or
// contains, a non-finite float -- see the package doc for why the
// formatting itself needs no hand-written logic, unlike on the Rust
// side. Exported at package level (not just within ElementKey) for the
// same reason collection.rs's own go_display is pub(crate): a future
// object-level walker's resolvePath-equivalent (the Go peer of F8,
// plan.rs, not yet written) will need the identical formatting for its
// own "{key=val}" path-segment matching, and must not grow a second,
// drifting copy of it.
func goDisplay(v interface{}) (string, bool) {
	if containsNonFinite(v) {
		return "", false
	}
	return fmt.Sprintf("%v", v), true
}

func containsNonFinite(v interface{}) bool {
	switch vv := v.(type) {
	case float32:
		return !finite(float64(vv))
	case float64:
		return !finite(vv)
	case []interface{}:
		for _, item := range vv {
			if containsNonFinite(item) {
				return true
			}
		}
	case map[string]interface{}:
		for _, item := range vv {
			if containsNonFinite(item) {
				return true
			}
		}
	}
	return false
}

func finite(f float64) bool {
	return !math.IsNaN(f) && !math.IsInf(f, 0)
}
