# Target-relative context

`reasoning::Rdfs12Context` runs selected source preparation and diagnostics before
adding context from an immutable original target. It is a Rust storage-adapter
API, separate from executable profile selection. The full `rdfs` selector remains
unsupported; this controller is a step toward its complete entailment procedure.

The controller calls `Rdfs12Source` first. `SourceConflicting` retains supported
source diagnostics and returns before the first target scan. Source errors abort
the whole attempt. This ordering is enforced by the operation, rather than an
externally supplied claim that source preparation already succeeded.

After source preparation, the controller traverses the original target for
canonical container vocabulary, inspects and types its literal appearances,
executes shared target-role resource typing, and closes the fourteen shared core
clauses. These phases reuse the existing semantic definitions and streaming APIs.
The target does not become source input, and source consistency is not rerun on
the augmented store.

The target occurrence phase uses `rdfs4` only. An arbitrary target predicate
does not gain `rdf:Property` merely by appearing in a target triple. Direct and
nested canonical `rdf:_n` terms select their shared ground vocabulary axioms;
literal datatype metadata does not count as an appearing IRI. No target triple or
quoted content is asserted. Target literal and occurrence evidence retains its
role, original root and structural path. Container axioms retain their shared
axiom authority, without fabricating an asserted target premise.

`Prepared { source, target }` reports completed selected phases. It is neither a
full consistency certificate nor an entailment verdict. Adequately substituted
target proposition candidates, construction interleaved with core consequences,
complete negative decisions and remaining conformance obligations stay separate.
A failed simple match must not be interpreted as negative RDFS entailment.

## Storage and failure contract

Start a fresh attempt over selected immutable assertions. Provide an immutable
original target with an independent blank-node scope. The source, target and
accessor must share one authoritative append-only term dictionary and cancellation
scope. Target scans are independently bounded and repeatable; cache eviction
reloads authoritative input or fails, never masquerading as absence. Generalized
scratch tuples and nested triple terms use the existing adapter contracts.

Caller-owned staging must retain source and target snapshot identities, profile
and catalogue versions, graph provenance and implementation identity. Keep target
context identified alongside its proofs, and check freshness and the complete
requested semantic contract before any atomic promotion. This operation does not
publish, promote or select source graphs.

Any storage, traversal, metadata, lexical, cancellation or resource failure
invalidates all partial scratch. A known-invalid target literal remains a
target-role error, rather than being relabelled as source inconsistency or a
negative entailment decision. A late target cursor error is still an error after
successful context writes.

## Explicit resource configuration

`Rdfs12ContextConfig` contains independent `source` and `target` configurations,
with no implicit defaults. Source limits retain the source controller's contract.
`Rdfs12TargetConfig` specifies:

- `axioms`: cumulative novel target-container vocabulary axioms.
- `traversal`: depth, appearances and work for each original-target traversal.
- `literals`: literal appearances, novel typing facts and lexical bytes.
- `structural`: core fixpoint limits and the occurrence phase's novel-output cap.
- `iri_bytes`: maximum owned container-descriptor metadata bytes.

There are three original-target traversals: vocabulary, literals and resources.
The subsequent core phase uses bounded immutable round snapshots. Backends also
enforce attempt-wide scratch, dictionary, metadata and I/O limits. Report totals
use checked arithmetic. Writable stores need only be `Send`; the future remains
`Send`, and borrowed term/error models do not gain a `'static` requirement.

The [context integration tests](../tests/rdfs_context.rs) cover ordering, nested
container selection, forbidden target assertion/predicate typing, lexical roles,
metadata isolation, late errors, budgets, identity checks and serialized reports.
The authored eighteen-case harness now has ten positive proofs and eight explicit
unsupported decisions. These measured results do not claim full RDFS conformance.
