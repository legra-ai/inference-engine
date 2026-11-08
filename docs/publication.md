# Alpha publication

The first release provides native checked rule-profile compilation and inspection.
It does not execute graph reasoning. Do not publish placeholders, advertise full
standards support, or make a stable release from this alpha pipeline.

## Version authority

Cargo workspace version `0.1.0-alpha.1` is authoritative. Rust and npm use that
spelling; maturin derives Python's equivalent `0.1.0a1`. Python metadata declares
Development Status 3 (Alpha); npm publication explicitly selects `alpha`, and
GitHub releases are prereleases. The release-version guard rejects stable,
beta/rc and nonzero-major versions. Promotion requires a deliberate policy change.

The existing shared prepare action advances the alpha candidate on releasable
main changes. Its cargo-version-info hook synchronizes npm metadata and lockfile;
maturin derives Python metadata directly from the same Cargo version.

## One tested revision

`.github/workflows/ci.yml` owns preparation, validation, source packaging, five
native platform builds, tagging and all three publications. Every downstream job
checks out the exact prepared revision. Publication requires the aggregate test
job, which requires all platform and artifact checks. PR builds never publish.

Rust integration tests run both in the checkout and the packaged crate. Python
wheels build from the source archive, then install into clean environments outside
the checkout. npm archives also install and test outside the checkout, without
install scripts. Missing native modules fail; no secondary evaluator is used.

Native platforms: Linux x64/arm64, macOS x64/arm64, Windows x64. Python Linux
wheels target manylinux 2.28. npm Linux addons build on Ubuntu 24.04 and require
its glibc baseline; musl is unsupported. Python requires 3.11+; Node requires 22+.
Both license texts ship in the Rust archive, Python source/wheels and npm tarball.

## Account setup

- GitHub: default `main`, squash merges, signed commits, owner review and required
  `prepare`, `test`, `Conventional Commit` checks. Keep inherited signature rules.
  Enable dependency alerts/security updates and daily Dependabot updates.
- Add this repository to the existing organization secrets' selected-repository
  access: `CRATES_IO_TOKEN`, `RELEASE_WRITER_APP_ID`, `RELEASE_WRITER_APP_KEY`,
  `RELEASE_MERGER_APP_ID`, `RELEASE_MERGER_APP_KEY`. Never copy secret values into
  source or create a second release writer.
- PyPI: pending GitHub publisher for project `inference-engine`, owner `legra-ai`,
  repository `inference-engine`, workflow `ci.yml`, environment `pypi`.
- npm: package `inference-engine`, trusted GitHub publisher for owner `legra-ai`,
  repository `inference-engine`, workflow `ci.yml`, environment `pypi`, direct
  publish permission. If npm requires an authenticated bootstrap publication,
  use the exact tested original tarball with `--tag alpha`; configure OIDC before
  resuming the same release. Do not add a parallel manual publisher workflow.

An empty GitHub repository needs its initial main revision before a PR can target
main. Follow the owner's explicit bootstrap authorization; never infer an
exception to the no-main-push rule from this document.

## Interrupted publication

The GitHub release stays draft until all registries are verified. Before publishing,
`release-packages.zip` preserves the npm archive, Python sdist and all five wheels,
plus semantic version, exact source revision and SHA-256 hashes. Never overwrite
that asset. A retry downloads and validates the originals, checks published bytes,
and selects only absent files; mismatches and authentication errors fail.

crates.io verification requires the actual downloaded crate's VCS revision to
match. npm verification compares the public tarball with its preserved original.
Python verification downloads the wheels and checks all recorded archive digests,
including the sdist metadata. Resume an unfinished release at its original tag
using the existing workflow before preparing a later release. Rebuilding a package
must never replace an already preserved publication artifact.

## Current setup evidence

Local Rust profile regressions, local native binding tests and packaged archive
checks are useful evidence, but are not proof of publication. Record live CI,
registry source/archive identity and completed GitHub prerelease before declaring
the first three-registry alpha published. Shared prerelease verification support
merged in legra-ai/github-actions PR 103 at
`436340e7d6c2b6d71921145415690b2518f1901d`; workflows pin that approved revision.
