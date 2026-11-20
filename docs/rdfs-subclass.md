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

`seed_existential` additionally executes the shared nonempty-class descriptors:
three `rdfD1a` requirements for the mandatory datatype map, plus `rdfs14a` for
RDFS propositions. Implement `Rdf12WitnessStore` to allocate a fresh scoped blank
once per descriptor and immutable attempt, persisting that binding in scratch.
Repeated seeding reuses its key and consumes no extra novel-output budget.

The witness is neither a fixed user IRI, a Skolem IRI nor a chosen lexical
representative. Different keys do not assert unequal denotations. Preserve
descriptor, exact ground membership, attempt/snapshot identity and origin as
semantic evidence; standardize witnesses apart from input/target blank scopes.
These seeds allow an empty source to entail existential class membership while
correctly withholding membership for an arbitrary fixed user IRI.

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

`inspect` completes the entire byte stream and returns `LiteralInspection` with
an optional first `LexicalViolation`. Metadata, UTF-8, read, cancellation and
budget failures remain errors, including failures after a lexical violation.
`validate` uses the same complete inspection and rejects that violation as
`LiteralError::Lexical`. Neither retains the whole payload. The
[source controller](rdfs-source.md) uses completed inspection to distinguish
source semantic conflicts from execution failures.

The caller owns total attempt budgets across literals. Literal inspection is a
prerequisite for datatype inference and consistency checking, not their complete
implementation.

## Executing literal typing

`Rdf12LiteralTyper` executes the three parameterized `GrdfD1` definitions from
`rdf-reasoning` using the same bounded pattern compiler and binder as the
structural executor. Its `LiteralOccurrenceCursor` streams a selected literal
key, its root triple, semantic role and an owned immutable `LiteralReader`.
Adapters may supply their own direct/recursive literal cursor, retaining
input-snapshot and logical-source-graph provenance. `Rdf12LiteralOccurrences`
provides the native traversal: implement `LiteralTermAccess` alongside
`EntailmentTerms`, select an immutable root cursor and explicit occurrence limits,
and the engine opens one literal reader at a time. Associated reader lifetimes
permit borrowed metadata and lexical slices or independent backend handles;
neither a whole lexical clone nor a dictionary/graph collection is required.

Each `LiteralOccurrence` retains its exact nonempty path as well as literal key,
root and role. Repeated keys at different paths remain distinct inspections.
Metadata datatype IRIs are not visited as actual terms. Opening must not buffer
the lexical payload; cache misses load authoritative classification/metadata or
fail. The reader then streams payload bytes through the existing validator.

`LiteralOccurrenceRead` distinguishes custom storage failures, native traversal
failures and literal-opening failures. `LiteralCursorError::Open` retains the
actual root, parent, path and role even when no payload reader could be opened.
`LiteralTypingError::Cursor` preserves that distinction. Backend failures and
late cursor errors must never become successful EOF or opaque nonliteral terms.
Borrowed term identities also retain the standard error-source contract without
requiring allocation or a static lifetime.

The typer validates metadata and streams the entire UTF-8 payload before
binding the auxiliary metadata relation. That relation is a distinct capability:
an RDF triple whose predicate spells its IRI cannot forge a metadata premise.
Unknown encountered datatypes remain opaque; graph assertions declaring them
datatypes do not extend the mandatory recognized map.

Select `LiteralRole::Source` or `TargetContext` explicitly. A role mismatch
fails before reading the payload. The caller must keep target-context seeding
separate from source-consistency analysis. Errors retain the literal occurrence
and role, so an invalid target literal cannot be mistaken for an inconsistent
source.

`LiteralTypingLimits` bounds aggregate occurrences, novel output and lexical
bytes for the whole phase. A zero remaining byte budget permits only empty
payloads, verified by an EOF probe. Duplicates consume inspection budgets but
not novel-output budget; asserted facts retain precedence. Cursor failures,
including errors after prior rows, abort the entire attempt rather than reporting
successful truncated output.

Generalized conclusions retain `Evidence::Literal` with the parameterized rule,
exact occurrence and validated byte count. They have no fabricated RDF premise.
This supplies literal-typing execution; graph-wide consistency and complete
RDFS 1.2 entailment remain separate work.

## Recursive occurrence traversal

`Rdf12OccurrenceCursor` streams actual term appearances from an immutable selected
root cursor using the asynchronous `EntailmentTerms` structural accessor. Every
`TermOccurrence` retains the exact term key, original root, source/target role
and nonempty `OccurrencePath`. Root and nested predicates are identifiable by
position. Repeated keys retain distinct paths and roots; triple terms themselves
and their nested components are visited without asserting the contents.

The caller attaches input snapshot and logical-source-graph provenance. Literal
metadata IRIs, language tags and direction are not traversed as actual RDF terms.
The same dictionary scope and exact target blank key must be preserved through
nested and outer occurrences; standardize target blanks apart from source blanks.

An iterative stack retains at most the configured path depth. `OccurrenceLimits`
bounds depth, emitted appearances and total root-read/term-lookup steps. There is
no graph/dictionary collection or recursive call stack. Cached-ready traversal
yields cooperatively. Term-access cache misses must reload authoritative structure
or fail, never return `None` for an evicted triple term. Late root failures,
unsupported term access, cancellation and exhausted limits abort the traversal
and invalidate partial semantic output. Futures are `Send`.

`Rdf12OccurrenceTyper` binds these trusted appearances to the shared `rdfD2`
and `rdfs4` definitions. It compiles their virtual premises and RDF heads through
the same checked pattern compiler as the structural executor. Source processing
admits both clauses; target context admits `rdfs4` only. A cursor/phase role
mismatch fails before traversal reads or clause compilation. Ordinary RDF triples
spelling an auxiliary relation cannot forge its semantic premises.

`Evidence::Occurrence` retains the rule and actual root, enclosing parent, path
and role. A nested parent is structural evidence, not an asserted RDF premise.
The typer caps novel output separately from cursor traversal budgets; duplicate
assertions/inferences consume inspection work but no novel-output budget. All
errors invalidate partial output. Writable stores need only be `Send`.

The phase processes one immutable selection. The controller must schedule newly
staged triples, literal validation, source consistency and target augmentation.
These phase APIs do not establish a complete RDF/RDFS entailment procedure.

## Finite proposition construction

`Rdf12PropositionConstructor` executes the shared three-premise `Grdfs14`
definition over exactly one immutable snapshot. It combines a property's typing
with subject/object resource membership and interns the resulting generalized
triple term through `Rdf12TripleTermStore`. Its contents are never asserted.
`Evidence::Proposition` retains the exact rule and all three premise tuples in
definition order; the caller also records this phase's scope, snapshot and origin.

Three bounded cursors and fixed bindings perform the join. `PropositionLimits`
caps examined rows and novel output, while the backend checks actual substituted
term nesting and dictionary limits. Generalized predicates, including literals
typed as properties by range inference, must be preserved or rejected explicitly
as unsupported. They must not be coerced through an IRI-only constructor.

The writable store need only be `Send`; the executor future remains `Send`
without requiring the store to be `Sync`. Only the immutable snapshot supports
shared reads.

Newly constructed terms never feed the same phase. The entailment controller
must choose adequate typing and source/target/substitution context before
construction. This operation does not establish a complete execution schedule
for the draft standard's postulated Appendix A procedure, and unrestricted
recursive construction is not permitted. All errors invalidate staged output.

Composition regressions execute source occurrence typing, finite construction
and simple matching together. They prove existing triple-term abstraction,
separate inner/outer abstraction, repeated term identity and consistent mappings
shared between nested and outer occurrences, with explicit negative cases.
No extra source blank is allocated for these abstractions, and no quoted contents
are asserted. These selected compositions do not prove completeness of every
source/target context or the complete controller schedule.

`Rdf12PropositionConstructor::run_candidates` provides targeted execution of
the same shared constructor. The controller streams candidate component tuples,
including already existing terms or target goals with adequate substitutions.
One immutable snapshot verifies all three exact typing premises; the same
canonical constructor, exact evidence, output cap and substituted-depth checks
apply. The engine does not enumerate the resource/property product in this mode.

Missing premises skip a candidate; they do not prove non-entailment. Candidate
selection completeness, target substitutions and phase scheduling remain the
controller's responsibility. Candidate rows and examined premise rows share the
strict work budget. Probes are exhausted to catch late read errors; rows outside
an exact requested probe fail with `ReasoningError::InvalidProbe`. Partial
staging is invalid on every failure. Writable stores need only be `Send`.

Some structural composition tests deliberately use generalized target triples.
A separate legal RDF 1.2 target regression uses triple terms only as objects:
source proposition-to-property subclassing plus `rdfs9`/`rdfs6` makes a newly
constructed proposition relate to itself through `rdfs:subPropertyOf`. Matching
that object shares a blank binding with an outer source relationship. The test
is false before construction and before the following core closure, then true;
a predicate mentioned only in target context remains a negative case. No Turtle
parser extension or quoted-content assertion is involved.

A further legal chained-goal regression constructs a first proposition, runs core
closure to establish its resource/property membership, then constructs a second
proposition containing it and closes core rules again. The second candidate is
unsupported before the intervening core phase, and the complete target remains
unmatched until both derivations finish. This establishes another scheduling
obligation; it does not justify unrestricted recursive constructor feedback or
certify a complete negative entailment procedure.

## Source datatype conflicts

`Rdf12DatatypeConflictChecker` executes the three shared positive diagnostic
clauses for the pairwise disjoint mandatory default datatype classes. It scans
one immutable source snapshot through two indexed cursors and writes each
`DatatypeConflict` to a caller-owned asynchronous `DatatypeConflictSink`.
The record retains the shared diagnostic ID, exact member key and both type
premises. These diagnostics are separate from RDF output and do not assert
negative membership or infer equality between different term keys.

Run this phase only after source lexical validation, nonempty datatype-witness
seeding and source closure, and before any target-context augmentation. Explicit
`LiteralRole::TargetContext` input fails before any reads or writes. The caller
still owns the actual source selection and must attach its immutable scope,
snapshot, catalogue and implementation identities to the diagnostic records.
The role guard cannot prove that the caller followed the required schedule.

`DatatypeConflictLimits` caps examined rows and diagnostic output. Cancellation,
late read failures, sink failures and exhausted limits abort the phase; discard
all partially staged diagnostics and RDF output. Cached-ready scans yield
cooperatively so other tasks can cancel the attempt. Futures remain `Send`
without requiring a writable store or sink to be `Sync`.

A successful zero-diagnostic report only means these three clashes were not
found in the supplied closure. It does not certify full datatype consistency,
RDFS consistency, unsupported datatype semantics or an integrated entailment
procedure. Nonempty-class witnesses are necessary to expose subclass clashes
even when the graph contains no literal instances.

## Source-to-target simple entailment

`entailment::SimpleEntailment` checks whether an immutable source graph contains
one instance of an immutable target graph. Both implement `TripleSource`, and
an `EntailmentTerms` adapter supplies asynchronous blank-node classification and
triple-term decomposition in a shared exact term-identity domain.

Target blank nodes are existential variables. One consistent substitution is
used across all target triples and inside nested triple terms. Different blank
nodes may map to the same term. The matcher backtracks across candidate source
triples and does not treat the contents of triple terms as assertions.

Source rows are streamed. Explicit limits bound retained target patterns, active
cursors, blank bindings, pending term pairs and total work. A target that exceeds
its budget fails with a typed error; it is never reported as non-entailed.
Storage errors and cancellation also abort the decision.

This is simple graph matching. To use it for RDFS 1.2 entailment, the caller must
first implement the complete target-relevant semantic procedure, including
datatype consistency and proposition handling. It does not compare datatype
values, derive missing closure or certify full RDFS conformance.

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
