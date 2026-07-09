# Contributing to SpecLance

Thanks for your interest in SpecLance. This is a small, single-maintainer
project that ships [Apache-2.0](LICENSE) Rust (and Python) tooling for
columnar, memory-mapped mass-spectrometry storage built on Lance.

Crates / packages in this repo: `speclance-core`, `speclance-ms`,
`speclance-cli`, `speclance-py`.

## Before you open a PR

- Open an issue first if the change is non-trivial (new API surface,
  on-disk schema change, vendor coverage, dependency bump beyond a
  patch). For small fixes - typos, docs, minor bug fixes, additional
  tests - go straight to a PR.
- Run `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings`
  locally. CI will run them too.
- Run `cargo test --all` (and `pytest` if the change touches Python).
- Update [CHANGELOG.md](CHANGELOG.md) under `## [Unreleased]` with a
  short bullet describing the user-visible change.
- Keep commits small and prefer [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `chore:`).
- Code is ASCII only and `#![forbid(unsafe_code)]` unless the crate
  explicitly opts in (none of the public crates currently do).

## Vendor ingest

SpecLance never talks to vendor formats directly - all vendor reads go
through `openproteo-io` and all mzML emission goes through
`openproteo-core`. If you need new vendor functionality, contribute it
upstream in [OpenProteo](https://github.com/Sigilweaver/OpenProteo)
first, then wire it through here.

## On-disk format

The Lance schema is considered an internal implementation detail until
v1.0. Breaking changes are allowed in 0.x releases but must be called
out in `CHANGELOG.md` and accompanied by a migration note in
`docs/`.

## Security

Please report security vulnerabilities privately via GitHub Security
Advisories - see [SECURITY.md](SECURITY.md). Do not open public issues
for vulnerabilities.

## DCO

By submitting a contribution you certify that you have the right
to submit the work under the project license (Apache-2.0) and
agree to the
[Developer Certificate of Origin](https://developercertificate.org/).

## License

By submitting a PR you agree that your contribution is licensed under
the Apache License 2.0, the same terms as the rest of the project.
