# Conformance corpus

Language-independent vectors. Each directory holds `input.yaml` and
`expected.yaml`; any implementation of this engine must agree with all of them.

This covers `reader/` and `canonicalize/` — `Stage::Read`/
`Stage::Canonicalize` semantics, YAML in, YAML syntax is the point.
`differential/` is a second, separate corpus layer with its own README,
its own JSON-only vector format and its own harness
(`engine/tests/differential.rs`, `go/*/differential_test.go`) — it
checks whether the F6-F9 primitives agree across languages given an
already-parsed value, a different question from whether two YAML
readers agree with each other. See `differential/README.md`.

`expected.yaml` states an `outcome` of `accepted` or `rejected`, and for a
rejection the stable `code` and `stage`. The human-readable `why` is context for
the reader and is **not** compared — a corpus that asserts on prose becomes a
corpus about prose.

`canonicalize/` extends this vocabulary for an accepted vector: `canonical`
pins the exact canonical JSON bytes section A requires (single-quoted, so a
value starting with `{`/`[` can't be misread as YAML flow syntax by a real
YAML parser), and the rarer `digest` additionally pins A7's `sha256:` value
for a vector where that is the point being tested — most accepted vectors
only need `canonical`, since the digest is a pure function of it. Both
fields are ignored for groups that don't declare them.

## Two rules the harness enforces

**A corpus with zero vectors cannot report success.** A gate nobody has shown
capable of failing is not a gate, and an empty directory is the easiest way to
have one by accident.

**Every group needs at least one accepted vector.** A checker that rejects
everything passes a corpus of rejections perfectly.

## Where these came from

`reader/duplicate_mapping_key` and `reader/alias_expansion` are the two cases
where the archived `cic-object-model`'s two implementations disagreed — Go
rejected a duplicate key while Rust silently took the last value, and on aliases
they disagreed in the other direction. Neither appeared in any vector of that
repository. They were found by running both implementations against the same
input, not by reading either.

That is why they are the first vectors here: a corpus proves what it contains,
and these are the cases nobody thought to write down.

`canonicalize/`'s twelve vectors (added once `engine/src/canonical.rs` existed
to run them against — PR #19/#21) are not from a two-implementation
divergence; they pin section A's own stated rules (key ordering, A3's exact
integer/float forms at both ordinary and extreme magnitudes, A4's escape
table including the characters RFC 8259 doesn't require escaped, a
canonicalization-stage rejection) so a future Go peer has the same fixtures
to agree with, not just this crate's own unit tests.

## Generated input, not only written vectors

The vectors above pin cases someone thought of. Two further checks cover the
rest (#54):

- `engine/tests/properties.rs` (Rust, `proptest`) checks over generated values
  and bytes that canonicalization never panics and writes valid JSON with
  sorted, unique keys, ignores insertion order, that the digest is a pure
  function of the canonical bytes, that the constructors refuse exactly what
  they promise to, and that the reader never panics. It runs in the gate at
  the default case count; `fuzz.yml` raises it on a schedule.
- The same suite writes a fixed-seed corpus (`export_cross_language_corpus`)
  that `go/canonical/crosslang_test.go` re-derives: for each generated
  document the Go peer must produce the same canonical bytes and digest. This
  is how the float tie divergence (#57) was found; a counterexample from
  either side becomes a vector in `canonicalize/` or `differential/`.
- `go/canonical/fuzz_test.go` is a native Go fuzz target (seeds run in
  `go test`; the long run is in `fuzz.yml`).

