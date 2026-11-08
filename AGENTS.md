# Agent instructions

- No compliments. Read README.md and docs/publication.md before changes.
- Write failing regressions first. Never use cargo check, bypass hooks or push main.
- Commit focused validated changes promptly. Preserve unrelated work.
- Use the pinned compiler and separate nightly formatter. Hooks own formatting/Clippy checks.
- Keep semantic rules/models in their existing shared crates; no duplicate evaluator.
- Stream graph-sized inputs and spill working state; finite rule descriptions may be bounded.
- Native binding failures fail immediately; no Python/JavaScript fallback engine.
- Alpha releases only until explicitly promoted. Cargo is the version authority.
- Use shared tested-main release actions; never add another publisher or expose credentials.
- Every major-number increase requires explicit approval of the exact version and scope.
- Check actual packaged tests/licenses and preserve tested archives across interrupted releases.
