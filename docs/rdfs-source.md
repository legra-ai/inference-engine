# Source preparation

`reasoning::Rdfs12Source` composes the available default-map source phases in one
bounded operation. It prepares a fresh selected asserted snapshot before any
source-to-target entailment context is introduced. Python and Node remain
profile-inspection APIs; this operation is Rust-only.

The operation first streams every literal appearing directly or recursively in
its original selected source. Recognized ill-typed literals are retained as
semantic diagnostics after their complete payload passes UTF-8 and transport
inspection through EOF. If any are found, the operation returns `Conflicting`
before axiom seeding or inference. A later cursor, read, metadata, cancellation or
budget failure aborts the attempt even if a lexical witness was already staged.

For a source without these lexical conflicts, the operation seeds shared RDF/RDFS/default-datatype axioms, scoped nonempty-class
witnesses and `rdf:_1`. Each source round discovers canonical container vocabulary
through direct and nested appearances, inspects literal metadata and streamed
payloads, types appearing terms/predicates, types existing triple terms through
exact shared constructor premises, and closes the fourteen core clauses. Rounds
repeat until no phase adds facts. Supported datatype-conflict diagnostics run only
after this source preparation and remain separate from RDF output.

`Rdfs12SourceOutcome::Prepared` means these selected phases finished without
those supported conflicts. It is not an entailment verdict or a certificate of
full RDFS consistency. `Conflicting` retains supported source diagnostics and
forbids promotion. Every error invalidates partial scratch output. Complete
requested semantics, input currentness and atomic publication remain caller-owned.
The [context controller](rdfs-context.md) now composes this operation with
original-target augmentation. Adequately substituted proposition candidates,
bounded goal scheduling and complete negative decisions remain outstanding.
The executable full `rdfs` selector remains unsupported.

## Adapter contract

Implement the existing asynchronous `ReasoningStore`, `Rdf12WitnessStore`,
`Rdf12TripleTermStore` and `LiteralTermAccess` capabilities, plus
`Rdf12ContainerAccess`. The latter returns a checked shared
`Rdf12ContainerProperty` descriptor for an actual appearing IRI, or `None` for a
supported noncontainer term. Its associated metadata lifetime permits borrowed
strings or shared handles; a full string clone is not required. Cap owned
container metadata before allocation. Cache misses reload authoritative data or
fail; they never mean an absent/noncontainer term.

Freeze the selected asserted facts for the entire attempt. Each round snapshot
freezes the current asserted-plus-staged fact relation. Store and accessor must
share one exact, stable term-identity domain and the same attempt cancellation
scope. Dictionary entries may append as axioms, blank witnesses and consequences
are staged; the accessor must see newly resolved keys without changing existing
identities. Never hydrate the whole dictionary or disguise an unread key as a
supported atomic term.

Container descriptors must resolve to the selected IRI key. Existing triple-term
decomposition must resolve back to the same canonical key. Identity mismatches
fail explicitly. Preserve generalized scratch tuples, including literal subjects
and predicates. No quoted contents are asserted, no arbitrary resource/property
product is generated, and no unrestricted constructor feedback occurs.

Implement `Rdf12SourceDiagnosticSink` for both source and context controllers.
Its `record` method accepts `Rdf12SourceDiagnostic::IllTypedLiteral` or
`Rdf12SourceDiagnostic::DatatypeConflict`. The first retains the exact literal
key, original root, recursive path, source role, complete byte count and first
shared lexical violation; the second retains both type premises. The standalone
pair checker continues to use its narrower `DatatypeConflictSink` capability.

The asynchronous diagnostic sink streams/spills records with source snapshot,
scope, catalogue and implementation provenance. A sink failure is a failed
attempt, not a zero-conflict result. Retain failures/diagnostics separately from
unpublished inference output.

## Lexical semantics and failure handling

The default map recognizes `xsd:string`, `rdf:langString` and
`rdf:dirLangString`. Shared `rdf-reasoning` definitions own their lexical spaces.
An `xsd:string` containing a scalar outside XML 1.1 `Char` has no denotation.
Syntactically legal language and directional strings have no ill-typed lexical
forms; unknown encountered datatypes remain opaque. Neither is diagnosed using
`xsd:string` lexical restrictions. These distinctions follow
[RDF 1.2 Semantics §7.1](https://www.w3.org/TR/2026/CRD-rdf12-semantics-20260924/#D_interpretations).

| Result | Controller behavior |
| --- | --- |
| Completed source lexical violation | Stage semantic evidence; finish preflight; return `Conflicting` |
| Completed source datatype-class clash | Stage both premises; return `Conflicting` |
| Invalid target literal | Target-context error; never source conflict |
| Read, UTF-8, metadata, cancellation, sink or resource failure | Error; invalidate partial scratch and diagnostic results |
| No supported conflict found | Continue selected phases; no full consistency certificate |

No arbitrary conclusions are materialized from an inconsistent source. Callers
must forbid promotion on `Conflicting`, and must not use partial diagnostics from
a failed operation as a completed source-check result.

## Explicit bounds

`Rdfs12SourceConfig` is a typed serialized configuration with no implicit defaults:

- `axioms` caps cumulative novel seeding across all rounds.
- `rounds` includes the final round proving no additions remain.
- `structural` caps each core fixpoint and the occurrence phase's novel output.
- `traversal` caps depth, appearances and read work for each immutable traversal.
- `inspection: SourceLiteralLimits` caps original source literal occurrences,
  semantic diagnostic records and total lexical bytes for the single preflight.
  Every occurrence counts, including repeated keys at distinct paths. Zero bytes
  permits only empty payloads, verified through an EOF probe.
- `literals` caps occurrences, novel output and lexical bytes per source round.
- `propositions` caps each existing-term candidate check; candidate count is
  separately bounded by traversal and round caps.
- `conflicts` caps the completed source-only diagnostic phase.
- `iri_bytes` caps one returned container descriptor's metadata.

Report literal and byte counters include preflight plus repeated round
inspections. A lexical-conflict report has zero rounds, seeds and inferred facts.
The round cap bounds total repeated phase work. Backends additionally enforce
attempt-wide scratch/dictionary/I/O budgets. Counters use checked arithmetic.
Writable stores need only be `Send`; the operation's future remains `Send`.
There is no graph-sized collection or filesystem work in this controller.

[The source integration fixtures](../tests/rdfs_source.rs) include an explicit
configuration and capped test adapter. They prove inherited membership, nested
literal/predicate/proposition processing, source-only container discovery,
nonempty-class conflicts, failure/non-promotion boundaries, canonical identity,
asserted precedence and JSON/Postcard round trips. This measured source coverage
does not replace the remaining complete RDF/RDFS entailment conformance work.

[Source literal consistency regressions](../tests/source_literal_consistency.rs)
also check nested evidence and JSON/Postcard round trips, late cursor failures,
preflight budgets, sink errors and datatype-specific lexical behavior.
