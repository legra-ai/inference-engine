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

**Graph evaluation, materialization, Oxigraph integration and SHACL inference are
not implemented in this alpha.** A compiled rule profile is not a claim of engine
capability, complete RDFS/OWL-RL support or consistency checking.

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
