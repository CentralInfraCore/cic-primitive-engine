# Conformance corpus

Language-independent vectors. Each directory holds `input.yaml` and
`expected.yaml`; any implementation of this engine must agree with all of them.

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
