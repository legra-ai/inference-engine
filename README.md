# inference-engine

[![crates.io](https://img.shields.io/crates/v/inference-engine.svg)](https://crates.io/crates/inference-engine)
[![PyPI](https://img.shields.io/pypi/v/inference-engine.svg)](https://pypi.org/project/inference-engine/)
[![npm](https://img.shields.io/npm/v/inference-engine/alpha.svg)](https://www.npmjs.com/package/inference-engine)
[![CI](https://github.com/legra-ai/inference-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/legra-ai/inference-engine/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE-MIT)

Native Rust inference foundation for RDF and Datalog, with Python and Node.js bindings.

## Alpha scope

The first alpha compiles named `rdf-reasoning` profiles into checked portable
Datalog programs and reports their explicit exclusions. Rust, Python and Node.js
use the same Rust implementation and published semantic dependencies.

The Rust API executes the explicit `rdfs-subclass` profile (shared rules `rdfs9`
and `rdfs11`) and `rdfs-core` (fourteen shared RDF/RDFS clauses). Callers supply
asynchronous database scans, indexed probes and scratch storage. Separate bounded
APIs seed RDF 1.2 ground axioms and scoped blank witnesses, validate streamed literal payloads, execute shared
default-map literal and recursive occurrence clauses, construct finite proposition
terms, report
source-only ill-typed literal and default datatype conflicts and check simple source-to-target entailment
with nested blank-node substitution. A [source-preparation controller](docs/rdfs-source.md)
composes these source phases before target augmentation. See also
[the individual adapter and execution contracts](docs/rdfs-subclass.md).
The [target-context controller](docs/rdfs-context.md) then processes original
atomic target resources, literals and container vocabulary after source diagnostics.
Whole triple terms without established resource facts are explicitly deferred
until construction establishes their denotation.
The [target-selected constructor](docs/rdfs-targets.md) now streams consistent
partial bindings to construct substituted target triple terms in finite passes,
interleaved with shared core closure by the caller.

**Full RDFS 1.2 entailment is in development.** Complete consistency checking,
adequate target-relevant construction and the integrated entailment procedure
remain outstanding. The development order is RDFS 1.2, OWL-RL,
general Datalog, then SHACL 1.2 reasoning. The other named profiles currently
support compilation and inspection. Python and Node expose profile inspection;
the executable storage-adapter APIs are Rust-only.

The Rust-only [SHACL 1.2 scheduling controller](docs/shacl12.md) is a separate,
partial execution slice. It orders shared checked schedule declarations, isolates
same-order phases and runs layers to a fixed point through native backend ports.
The original W3C same-order SPARQL fixture executes in a test adapter. A complete
SHACL compiler, production backend integration and Python/Node execution APIs
remain outstanding; this is not full SHACL 1.2 support. Existing SHACL-AF
compatibility in consumers remains a distinct mode.

```rust
let profile = inference_engine::compile_profile("rdfs-core")?;
assert_eq!(profile.program.rules.len(), 14);
# Ok::<(), Box<dyn std::error::Error>>(())
```

```python
from inference_engine import compile_profile
profile = compile_profile("rdfs-core")
print(profile["exclusions"])
```

```javascript
const {compileProfile} = require('inference-engine');
console.log(compileProfile('rdfs-core').exclusions);
```

Unsupported selectors fail; full `owl2-rl` is not an alias for a restricted profile.
Python requires 3.11+; Node.js requires 22+. Native artifacts target Linux glibc
x64/arm64, macOS x64/arm64, and Windows x64. No Python or JavaScript evaluator is
used when the native module is missing.

## Conformance evidence

The packaged tests include a pinned W3C RDF 1.2 semantics manifest and fixtures
with original hashes and license notices. The simple-entailment harness passes
18 cases; it explicitly reports the case requesting `xsd:integer` value
comparison as unsupported. Stronger RDF/RDFS regimes and inherited RDF 1.1
semantic tests are not counted as passing by that harness. This is measured
partial coverage, not full RDF 1.2 or RDFS conformance.

A separate harness executes the eighteen authored RDF 1.2 regression cases
published in `rdf-reasoning` 2.17.0. It parses every source and target with
independent blank-node scopes, then uses the native ordered source/context and
simple-matching APIs, with an explicit two-pass construction/core witness schedule.
Twelve cases have positive proofs; six remain unsupported pending a complete
RDFS decision procedure. Unsupported cases are reported separately from failures
and execution errors, and are never counted as negative entailment proofs.
The original manifest, hashes and both licenses are included in the crate.

## Development and releases

Rust is pinned in `rust-toolchain.toml`; formatting has an independent nightly
pin. `cargo test --locked` runs public API tests. Commit hooks run formatting and
Clippy. `python3 scripts/test-release.py` checks the release contract.

Cargo is the version authority: `0.1.0-alpha.1` maps to Python `0.1.0a1` and npm
`0.1.0-alpha.1`. All three publish from one tested revision. npm uses the `alpha`
dist-tag; GitHub releases remain prereleases. Stable promotion is a deliberate
future change. See [publication setup](docs/publication.md).

## License

Licensed under either the Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
or the MIT license ([LICENSE-MIT](LICENSE-MIT)), at your option.
