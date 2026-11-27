# SHACL 1.2 scheduling controller

`shacl12::Controller` implements the scheduling loop of the
[10 November 2026 SHACL 1.2 Inference Rules draft](https://www.w3.org/TR/2026/WD-shacl12-inference-rules-20261110/).
It is a partial Rust-native execution slice, not a full SHACL processor.

## Checked programme and ownership

A native compiler preflights the entire requested programme before creating a
`Plan`. It selects supported rule types, resolves global versus shape-rule status,
deduplicates identities shared by multiple shapes, removes deactivated rules and
rejects unsupported processors, conditions, expressions and entailment requests.
No unsupported construct may be silently ignored or evaluated as SHACL-AF.

Each `Rule` associates a backend-owned compiled handle with the existing
`shacl-structural-model` `RuleSchedule`. The explicit programme count cap applies
before sorting. Rule identities must be unique. The shared model preserves exact
integer/decimal lexical identity and compares numeric values without floats.
Absent layer/order declarations take the standard zero default; absent run-once
is false. The model's declarations pin the 28 October draft; the scheduling fields
used here are unchanged in the selected 10 November execution contract. This
reuse does not claim coverage of the model's excluded expression forms.

Compiled handles and plans are runtime resources, separate from portable Serde
models. Terms and graph-scaled relations stay in canonical backend storage. The
controller retains only the capped programme and constant-sized counters. It does
not depend on Oxigraph, another database, or a particular asynchronous runtime.
Oxigraph is a dev dependency used to execute the original SPARQL test queries.

## Execution and snapshot boundaries

For each ascending numeric layer, the controller calls `begin_layer` once with
that layer's exact rule slice. The backend computes expected derived triples
before taking rule-execution snapshots. Run-once rules execute first, in numeric
order. Iterating rules then execute in numeric order until a whole iteration
adds no novel inferred triples. Same-order rules share one immutable snapshot;
the complete pending phase merges only after every rule in that group succeeds.
Lower-order additions are visible to the next group and later iterations.

`RuleBackend` supplies immutable query/dataset snapshots in addition to the
streaming/probed triple-read capabilities of existing storage adapters. Snapshot
handles must retain their pinned meaning after later writes. They are not
permission to hydrate a graph or flatten a dataset. The caller explicitly pins
base and shapes graph identities, graph scope and attempt identity. All original
inputs remain immutable. Pending phases and inferred output require separately
queryable indexed scratch that can spill; they are never controller collections.

The backend executes precompiled native rules. Global execution has no focus
binding. Shape-rule execution streams the deduplicated union of active target
nodes and checks all active conditions. A run-once rule shared by two shapes must
execute only once per focus node. The controller supplies an attempt-local phase
ordinal; the backend retains actual rule/focus/source/shapes/phase evidence,
profile and implementation revisions. Do not fabricate RDFS rule IDs or asserted
premises for SHACL output.

After a layer's fixed point, `finish_layer` removes derived-only triples and their
annotations, retaining independently inferred triples. `finish` removes temporary
triples and annotations. These callbacks are required native capabilities, not
optional fallbacks: a compiler must reject constructs its adapter cannot execute.

## Failure, limits and publication

Limits cap iterations per layer, charged attempt-wide work and novel inferred
triples. Iteration counts include the final empty round. Novel-output counts
include temporary output before cleanup. Backends charge streamed rows, target
nodes, expression work and candidate triples before processing them. Work
exhaustion is sticky even if a backend accidentally ignores the charge error.
Backends additionally enforce query-internal, dictionary, nesting, storage/I/O
and term-creation limits and cancellation. The controller cooperatively yields
independently of backend work-charge granularity and checks cancellation at native
operation boundaries.

A backend error, exhausted budget or cancellation invalidates the **whole
attempt**, including earlier merged phases. No successful report is returned.
Callers must discard or abort all attempt-owned staging; this controller neither
publishes nor rolls partial output into the caller's graph. After success the
caller still validates snapshot currentness and atomically promotes its own
inference contribution. A successful report alone grants no publication authority.

## Evidence and outstanding integration

`tests/shacl12_w3c.rs` reads the unmodified W3C `same-order.ttl`, extracts its actual
CONSTRUCT strings and expected result list, and runs the queries through an
Oxigraph test adapter under the native controller. Provenance records the exact
suite revision and original SHA-256; the fixture, W3C license and notice are
packaged. This tests one scheduling case, not all SHACL syntax or conformance.
The test adapter is finite-fixture infrastructure, not a production adapter.

The remaining work includes the production preflight compiler, full native
query/target/condition/expression adapters, derived/temporary/provenance lifecycle,
rule-set and entailment integration, the complete pinned W3C Infer suite and
explicit Python consumer invocation. Python and Node bindings currently expose
profile inspection only. Existing pySHACL-compatible/AF calls must retain their
behavior when the new mode is added; both paths require regression verification.
