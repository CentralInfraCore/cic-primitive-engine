# PrimitiveIR

The engine's output, and the only thing generators may consume.

> **Not specified yet.** This file states what the IR must satisfy, so that the
> constraints are fixed before the representation is. Writing the representation
> first is how the previous attempt ended up with a model that could not hold
> its own version number.

## Required properties

**Versioned.** Every IR document declares its version. A consumer that has not
declared support for that version must not be handed it.

**Canonical.** One byte representation, defined by this repository — not
whatever `serde_json`, `json.dumps()` or a YAML emitter happens to do today. The
archived model discovered late that two implementations agreed semantically on
all thirteen materialization vectors and matched on **zero** of them byte for
byte, because canonicalization was never written down.

**Complete.** No member is optional-by-omission. If a value is absent, the IR
says so explicitly and says why — authored-absent, not-observed, not-implemented
— and these are distinct.

**Traceable.** Every value carries, or is accompanied by, where it came from:
authored, schema default, derived. See [BOUNDARY.md](BOUNDARY.md#the-receipt).

**Closed.** There is no member the engine passed through without interpreting.
"I processed this document in its entirety" must be a statement the engine can
make and a consumer can check.

## Open questions

1. Is the receipt part of the IR document or a sibling artifact bound by digest?
2. Does the IR carry unresolved references explicitly, or is resolution total?
3. What is the canonical form — a YAML profile, canonical JSON, or something
   the engine defines outright?
4. How does an IR version relate to the `cic-primitives` schema version? One
   number or two?
5. Which parts are stable API and which are engine-internal?

Each of these has to be answered before the stages above `Read` are written.
