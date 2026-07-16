# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Dependency renamed `openproteo-io` -> `openmassspec-io` (1.0.0),
  following the umbrella's rename from OpenProteo to OpenMassSpec.
  No behavioral change.

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
