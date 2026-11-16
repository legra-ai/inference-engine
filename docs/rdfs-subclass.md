# Executable RDF/RDFS structural profiles

`reasoning::ExecutionProfile::RdfsSubclass` selects exactly `rdfs9` and `rdfs11`
from the pinned `rdf-reasoning` catalogue. Its selector is `rdfs-subclass`,
profile revision is `1`, and its rule definitions are evaluated as Datalog
patterns rather than reimplemented in the storage adapter.

Given:

```turtle
ex:Person rdfs:subClassOf ex:Agent .
ex:Agent rdfs:subClassOf ex:Thing .
ex:alice rdf:type ex:Person .
```

the inferred additions are:

```turtle
ex:Person rdfs:subClassOf ex:Thing .
ex:alice rdf:type ex:Agent, ex:Thing .
```

No reverse subclass implication is made. Cycles can imply self-subclass
relationships and terminate through exact set membership. Asserted triples are
not copied into inferred output, even when a rule can also prove them.

This profile excludes **every other rule**, axiomatic triples, datatype
reasoning, subproperty/domain/range reasoning, OWL reasoning, contradiction
detection and SHACL. It does not claim complete RDFS entailment. Existing
`ExecutionProfile::RdfsCore` additionally executes all fourteen shared flat
RDF/RDFS clauses, including subproperty inheritance, domain and range. Its exact
rule selection and exclusions come from `rdf-reasoning::Preset::RdfsCore`.
Neither structural selection includes axioms or full datatype/triple-term
semantics. The executor rejects `rdfs`, `owl2-rl` and unknown selectors instead
of weakening them.

## Explicit RDF 1.2 axioms

`Rdf12AxiomSeeder` stages shared standard ground facts into the same fresh
attempt before structural execution. `seed_family` takes `Rdf12AxiomId::Rdf`
(nine facts), `Rdfs` (forty-one) or `DefaultDatatypes` (three). The last family
declares `xsd:string`, `rdf:langString` and `rdf:dirLangString` as datatypes; it
does not validate literal payloads or establish consistency.

`seed_container` takes a checked `Rdf12ContainerProperty`, supplied one IRI at a
time, and stages its four RDF/RDFS axioms. Canonical positive indices can exceed
machine-integer ranges. The caller selects relevant vocabulary from source and
target, including `_1` when required by the standard's entailment procedure.
The seeder never enumerates the infinite container vocabulary.

The seeder's explicit novel-output cap applies cumulatively across all its
calls. Duplicates consume no output budget. Structural execution has a separate
cap: the adapter must also enforce total attempt-wide storage and work limits.
Any seeding error invalidates the entire attempt, including earlier staged facts.

This API executes finite axiom definitions from RDF 1.2 Semantics CRD
2026-09-24. Full source-to-target RDFS 1.2 entailment additionally requires
datatype validation and consistency, recursive term/proposition semantics, and
existential matching. Finite seeding plus fourteen rules is not a conformance
claim.

## Streaming literal inspection

`Rdf12LiteralValidator` checks one literal through an adapter's `LiteralReader`.
The reader exposes borrowed `rdf-structural-model` metadata independently from
its asynchronous lexical byte stream. Metadata is checked before the payload is
read. The validator uses 4096-byte reads and at most three carried UTF-8 bytes,
with an explicit per-literal byte budget and cancellation/error propagation.

Recognized lexical semantics come from `rdf-reasoning::Rdf12Datatype`. The
mandatory map contains `xsd:string`, `rdf:langString` and `rdf:dirLangString`.
String validation uses XML 1.1 character ranges. Language and directional
strings accept every valid UTF-8 lexical payload after their metadata passes
structural validation. Encountered unknown datatype IRIs remain opaque.

Successful reports include the exact byte count and recognized datatype, if any.
Lexical errors preserve the shared diagnostic plus the fragment's global start
offset. The adapter retains the original literal key and occurrence provenance;
validation never canonicalizes its lexical form, language spelling or direction.
Backend read failures, malformed UTF-8, unsupported metadata, cancellation and
resource exhaustion stay distinct from recognized lexical violations.

The caller owns total attempt budgets across literals. Literal inspection is a
prerequisite for datatype inference and consistency checking, not their complete
implementation.

## Rust use

```rust
use inference_engine::reasoning::{
    ExecutionProfile, Reasoner, ReasoningError, ReasoningStore, RunLimits, RunReport,
};
use std::num::{NonZeroU32, NonZeroU64};

async fn infer<S: ReasoningStore>(store: &mut S)
    -> Result<RunReport, ReasoningError<S::Error>>
{
    let limits = RunLimits::new(
        NonZeroU32::new(128).unwrap(),
        NonZeroU64::new(1_000_000).unwrap(),
        NonZeroU64::new(100_000_000).unwrap(),
    );
    Reasoner::new(ExecutionProfile::RdfsSubclass, limits).run(store).await
}
```

These limits are examples; select explicit budgets appropriate to the input.
`rounds` includes the final round that proves no additions remain. `matches`
counts cursor rows examined across all scans, not just successful rule matches.
Output-budget failure occurs before staging a tuple beyond the configured cap.

## Database adapter

Implement `ReasoningStore`, `TripleSource` and `TripleCursor`. Their exact Rust
signatures live in `src/reasoning/mod.rs`; the public regression fixture
`tests/subclass.rs` is a small, capped test adapter, not a production graph store.

- `Term: Clone + Eq + Send + Sync` can be a database ID or an exact backend RDF
  term. `Copy`, strings and whole-dictionary conversion are not required.
- Tuples are `[Term; 3]` in subject/predicate/object order. Patterns are
  `[Option<Term>; 3]`, where `None` means wildcard. Match bound terms exactly.
- `resolve_iri` resolves full shared-rule constants as **IRI terms**. Preserve
  blank-node scope, literal lexical identity, language/datatype metadata and
  any backend term identity; the engine does not parse or normalize these.
- Start a fresh attempt containing the immutable selected **asserted** input
  and empty inferred scratch. Never seed it with a previous inferred closure.
- `snapshot` returns an **owned immutable round view** of selected assertions
  plus staged inference at that moment. Its cursors must survive later writes
  to the separate store. New insertions join only in subsequent rounds.
- `scan` opens a bounded fallible cursor with indexed probes. Two cursors can
  be active per rule join. `next` returns one tuple, `None` only at successful
  EOF, and a typed backend error on read/cancellation failure. Dropping a cursor
  releases/cancels the read. Backend futures and the executor future are `Send`.
- `contains` probes the **current** asserted-plus-staged union, including facts
  inserted since the round snapshot. `insert_inferred` deduplicates atomically,
  gives assertions precedence, and returns true only for a novel inferred fact.
- `Inference<T>` includes typed `Evidence`. A `Rule` witness contains the shared
  `RuleId` and `Premises::One` or `Premises::Two` in definition order. An `Axiom`
  witness records its exact standard family; its ground inference tuple completes
  the identity. Axioms have no invented premise or rule identifier.
  A rule premise can itself be inferred. This is a first witness, not every proof.
  Scan order can change that witness; exact inferred set closure is independent
  of order. Do not treat the witness as a canonical provenance hash.
- Keep graph-sized scratch relations and deduplication indexes in disk-backed
  storage. No engine method returns a closure `Vec`. The caller separately
  budgets scratch space, term payloads, I/O and backend queues.
- Preserve generalized tuples in scratch; do not hide an unsupported term or
  backend operation by silently dropping it. A standard RDF output adapter must
  validate its representational support before beginning the attempt.

The engine does not select graph scope. A caller can explicitly choose a union
of selected logical source graphs, or run separate stores for graph-local
reasoning. Do not expose unrelated/control/document/inferred graphs, or mistake
physical contribution graph IRIs for logical source graph identities.

## Completion and provenance

`run` returns `RunReport` only after a fixpoint. Cancellation, a read/write error,
unsupported rule shape or any limit returns an error. Some staged facts can
exist on failure; **abort them and never publish a partial closure**.

The caller owns promotion. Record selected input snapshot/graph identities,
`profile().name()`, `profile().revision()`, `profile().rules()`,
`implementation_version()`, `rule_catalogue()` and configured limits. Check that
the selected input is still current, then atomically promote the owned inference
contribution. A success report alone is not a currentness or publication guard.
For explicit axiom seeding also record selected families, container vocabulary
selection and the seed budget. The catalogue identity is generated from the
exact dependency pin in the packaged manifest.

On asserted-input deletion/change, profile change or implementation change,
recompute from a new asserted snapshot and atomically replace the contribution.
Keep assertions separate. Render inferred relationships with explicit origin,
preserve asserted precedence on duplicates, and retain inference/provenance in
KG export so restored source-free rendering does not rerun inference.

Python and Node distribution packages currently expose profile inspection;
this storage-adapter execution API is Rust-only.
