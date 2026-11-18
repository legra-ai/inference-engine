# Target-selected proposition construction

`reasoning::Rdf12TargetConstructor` selects substituted original-target triple
terms for the existing shared Grdfs14 constructor. It is one finite positive
search phase. Complete RDFS 1.2 entailment remains in development, and the full
`rdfs` executable selector remains unsupported.

Run `Rdfs12Context` over a fresh selected source and an independently scoped,
immutable target first. Continue only after `Prepared`; retain source conflicts
and abort errors under that controller's contract. The constructor does not
repeat source preparation or run source consistency on target-augmented scratch.

## Selection and shared execution

The phase freezes one snapshot for both partial matching and all constructor
premises. It streams source scans/probes while retaining only explicitly capped
target patterns, active cursors, blank bindings and recursive worklists. It
enumerates consistent bindings from matched subsets of the target, including the
empty subset. The existing simple-entailment unifier handles nested terms and
repeated blanks. A skipped pattern is a search choice and supplies no proof.

Each binding substitutes the original target's triple-term templates. An unbound
blank receives the single `rdf:type` representative; distinct target blanks may
share that representative. Already bound variables retain their exact values.
This selects concrete candidate witnesses; it does not enumerate every possible
RDFS interpretation or prove selection adequacy. No target predicate becomes a
property merely by appearing in a target.

Grounding traverses templates iteratively, including nested triple terms. It
resolves canonical generalized keys and verifies their decomposition. A term
whose components are unchanged must retain its original key. Dictionary entries
alone supply no denotation or typing premise. The shared constructor still checks
the exact resource/property/resource premises for every candidate, against the
fixed phase snapshot, before emitting proposition membership. Unsupported
candidates emit no fact. Neither target triples nor quoted contents are asserted.

New facts cannot feed matching or constructor premises in the same pass. Close
the existing shared `RdfsCore` rules before any explicitly selected later pass.
The coupling regression needs one construction/core cycle; the chained regression
needs two. A separate consistent match of the entire target supplies the positive
proof. Constructor reports alone are not entailment verdicts.

## Bounds, evidence and failures

`TargetConstructionLimits::new` requires four independent settings:

- `EntailmentLimits` bounds retained target patterns, active blank bindings,
  unification worklists and total matching/probe work.
- `PropositionLimits` bounds cumulative candidate/premise rows and novel output
  across the complete pass, rather than resetting per substitution.
- `slots` bounds the combined pending grounding tasks and values. Each task owns
  at most four bounded-clone term handles; no dictionary or relation is hydrated.
- `work` bounds total template inspection and grounding steps across substitutions.

The backend must additionally enforce attempt-wide scratch, dictionary, metadata,
actual substituted nesting, I/O and cancellation limits. Source graphs remain
streamed. Matching can examine exponentially many subsets, so configured work
caps are required even for a small target. Cooperative checkpoints run throughout
cached reads, matching and grounding.

`TargetConstructionReport` reports substitutions, matching steps, grounding steps
and the shared constructor's counters. `Evidence::Proposition` retains the shared
rule and its three actual typing premises. Callers also retain original source,
target and phase snapshots, profile/catalogue/implementation versions, graph
provenance and the selected scheduling configuration. No target assertion is
fabricated as a premise. Callers own freshness and atomic promotion.

Any storage, identity, malformed-probe, cancellation or exhausted-cap error
invalidates the entire scratch attempt. Late failures remain errors after useful
reads or writes. No error or zero-output report becomes negative entailment.

## Measured scope and remaining procedure

The original eighteen-case authored fixture harness uses the same fixed schedule
for every case: ordered source/context preparation, whole-target matching, and
up to two construction/core cycles with matching after each. Expected verdicts
never select phases or substitutions. Twelve cases have positive proofs and six
remain explicitly unsupported; none of those six is reported as a negative RDFS
decision. These are authored regressions, separate from official W3C conformance.

Two passes are a measured positive-witness schedule, not a fixpoint or a complete
entailment algorithm. An unmatched target can cause deeper terms to be generated
on later passes. A separate regression demonstrates that the third pass of the
conflicting inner/outer blank case reaches the backend's substituted-depth limit
and returns the actual error. Do not turn this operation into an unrestricted
constructor-feedback loop or interpret a finite unsuccessful schedule as a
negative answer. Complete consistency and a justified adequate finite decision
procedure remain outstanding before full RDFS support.
