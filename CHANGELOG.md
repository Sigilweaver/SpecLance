# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Updated `lancedb` from 0.31.0 to 0.41.0 (Lance 8 -> 14, DataFusion 53 ->
  54). 0.40.0 was skipped because it does not compile without lancedb's
  `remote` feature. No SpecLance source changes were needed and store
  behaviour is unchanged: stores written by the previous version open,
  query, append and re-index under the new one, and a new
  `store_contract` test suite (on-disk schemas, field round-trips, scan
  ordering, inclusive and open RT bounds, null RTs, quote escaping in
  run-id filters, and indexed vs unindexed query results) passes on both
  the old and the new stack.
- All Arrow crates now resolve to a single version, 58.4.1 (previously
  `arrow-array`/`arrow-schema` 58.3.0 alongside an unused `arrow-select`
  59.2.0, so two Arrow majors were compiled). Arrow stays on major 58
  because Lance pins it; the unused direct `arrow-select` dependency is
  removed. Dependabot now groups `arrow*` and `lance*` updates so the set
  moves together.
- Updated the locked `opentfraw` (via `openmassspec-io`) from 1.4.0 to
  1.6.0. Lockfile-only change.
- `rust-version` is now 1.95, the highest minimum among the dependencies
  (`openmassspec-io`, `opentimstdf`; Lance 14 needs 1.94). The previous
  1.75 was already below what the dependency tree required.
- CI builds dev/test profiles without debuginfo (`CARGO_PROFILE_DEV_DEBUG=0`).
  With Lance 14 the debuginfo for the workspace test binaries no longer fits
  on the ubuntu-latest runner's disk.
- `cargo audit` in CI no longer ignores RUSTSEC-2026-0194/0195: the
  `quick-xml` 0.26 they flag came from Lance's `pprof` dependency, which is
  gone from the tree with Lance 14.
- Updated `openmassspec-io` to 1.5.4 (which pulls `openmassspec-core` 1.5.0)
  and adopted its expanded run and precursor metadata API. The new core
  `SpectrumRecord`/`RunMetadata` `extra` and `acquisition_event_id` fields
  are defaulted in the mzML writer (empty / `None`); no field is populated
  yet, so output is unchanged.
- Dependency renamed `openproteo-io` -> `openmassspec-io` (1.0.0),
  following the umbrella's rename from OpenProteo to OpenMassSpec.
  No behavioral change.
- CI now runs the Rust build/test job and the Python bindings job on
  macOS and Windows in addition to Linux, matching the wheel matrix
  already used by the release workflow. Verified with a local build
  and test pass on Linux; the macOS/Windows runs have not been
  observed in Actions yet.

### Security

- Updated the transitive `zstd` 0.13.4, `zstd-safe` 7.4.0 and `zstd-sys`
  2.1.2 (via `arrow-ipc` and `lance-encoding`) in the lockfile, fixing
  RUSTSEC-2026-0391, RUSTSEC-2026-0398, RUSTSEC-2026-0403 and
  RUSTSEC-2026-0410, plus the `zstd-safe` unsoundness warnings
  RUSTSEC-2026-0395 through -0397, -0399 through -0402 and -0404 through
  -0406. Also updated `event-listener` to 5.4.2,
  clearing the RUSTSEC-2026-0221 unsoundness warning. Lockfile-only change.
- Updated locked dependencies `rustls` 0.23.46, `rustls-webpki` 0.103.16,
  `quinn-proto` 0.11.19, and `chacha20` 0.10.2, fixing RUSTSEC-2026-0338
  through 0342, RUSTSEC-2026-0346 through 0349, and RUSTSEC-2026-0382, and
  clearing the yanked `chacha20` warning. Lockfile-only change.

- Updated the locked TLS dependencies to `rustls` 0.23.45 and
  `rustls-webpki` 0.103.15, fixing RUSTSEC-2026-0285. The existing
  `h2` 0.4.16 security fix and documented audit exceptions are preserved.

- Bumped `quick-xml` from 0.36 to 0.41 and `pyo3` from 0.22 to 0.29,
  clearing the RUSTSEC-2025-0020, RUSTSEC-2026-0177, RUSTSEC-2026-0194,
  and RUSTSEC-2026-0195 advisories. `cargo-audit` no longer needs the
  `--ignore` flags that were tracking these.

### Documentation

- Added a Python API reference page (`guide/python-api`) to the docs
  site, covering the `speclance` package's `Store` class (`open`,
  `ingest_mzml`, `create_default_indexes`, `runs`, `query_window`,
  `chromatograms`). (#2, contributed by @Nabejo)

## [0.2.0-alpha] - 2026-05-31

First public alpha. SpecLance moves from `develop` to `main` as the
default branch and the workspace is ready for publication on crates.io
and PyPI.

### Added

- `speclance-core` crate: Lance-backed `Store` with per-run dataset
  layout, `RunMetadata` / `SpectrumRecord` types, scalar indexes on
  retention time and m/z, and a range-query API.
- `speclance-ms` crate: streaming `MzmlIngest` cursor, mzML 1.1.0
  reader/writer with full roundtrip support, and a `vendors` feature
  family (`thermo`, `bruker`, `waters`, `all-vendors`) that turns on
  the matching `openproteo-io` feature set.
- `speclance-cli` crate: ingestion dispatch by file extension
  (`.raw` file -> Thermo, `.raw/` dir -> Waters, `.d/` dir -> Bruker,
  `.mzML` -> mzML), Lance store management, and `speclance export` to
  emit mzML back out of a store.
- `speclance-py` crate: PyO3 bindings (`_speclance`) exposing `Store`,
  ingest, indexing, and range-query against PyArrow / Polars / Pandas.
- CI: cross-platform (ubuntu / macos / windows) Rust workflow plus a
  dedicated Python wheel job.
- `CITATION.cff`, `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`.
- `[workspace.lints]` block forbidding `unsafe_code` across the
  workspace.

### Changed

- All vendor ingest now routes through `openproteo-io` rather than
  calling vendor readers (`opentfraw`, `opentimstdf`, `openwraw`)
  directly. SpecLance no longer maintains per-vendor adapter code -
  vendor coverage tracks the OpenProteo stack one-for-one.
- mzML emission delegated to `openproteo-core`'s streaming writer
  (indexed output, SHA-1 footer).
- Workspace manifest: `homepage = "https://sigilweaver.app/speclance/"`,
  `keywords` and `categories` added for crates.io discoverability
  (WP13).
- README badge block unified with the rest of the Sigilweaver
  portfolio; "Part of the OpenProteo stack" callout added.
- CI now checks out the `OpenProteo` sibling repository so the
  `openproteo-io` path dependency resolves.

### Fixed

- References to the old `OpenTDF` name updated to `OpenTimsTDF`
  across CI, manifests, README, and tests.

## [0.1.0] - 2026-05-17

Initial scaffold (never published). Cargo workspace, core store
prototype, README, gitignore.
