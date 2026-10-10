# Changelog

All notable changes to this project will be documented in this file.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- mzML export no longer drops the activation method for spectra ingested
  from mzML. The reader stores short names (`HCD`, `CID`, `ETD`, `PQD`)
  and the writer only recognised PSI-MS long names, so those spectra were
  exported with no activation. The writer now accepts both forms.
- Ingesting a run that is already in the store is now refused with a
  clear error (`Error::RunExists`) instead of appending a second copy of
  its rows, which produced duplicate spectra and invalid mzML on export.
  Applies to the CLI, the Python `ingest_mzml`, and `Store::append_run`.
  New `Store::contains_run` and `Store::ensure_run_absent` helpers.
- `batches_to_runs`, `batches_to_spectra`, and `batches_to_chromatograms`
  now return `Result` and report a missing or mistyped column as
  `Error::Schema` instead of panicking. This is a breaking change to their
  signatures.
- The mzML reader reports malformed input instead of guessing: a
  non-numeric spectrum `index` attribute (was read as scan 1), an
  undecodable `<binary>` text node (was read as empty), and a named
  entity inside `<binary>` (was dropped) are now `MsError::Malformed` /
  `MsError::Xml`.
- Vendor ingest fails with an IO error when the bundle size cannot be
  read, instead of hashing a smaller size into the run ID.

### Changed

- MSRV raised to Rust 1.95 (`rust-version` was 1.75, which the `lance` /
  `lancedb` / `openmassspec-io` dependencies already exceeded).
- Removed the unused `arrow-select` dependency.
- Dependabot now groups `arrow*` and `lance*` updates into one PR, since
  they must move together.
- CI no longer checks out the OpenTFRaw, OpenTimsTDF, OpenWRaw, and
  OpenMassSpec sibling repositories (all are resolved from crates.io) and
  no longer triggers on the nonexistent `develop` branch.

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

- Updated the locked TLS dependencies to `rustls` 0.23.45 and
  `rustls-webpki` 0.103.15, fixing RUSTSEC-2026-0285. The existing
  `h2` 0.4.16 security fix and documented audit exceptions are preserved.

- Bumped `quick-xml` from 0.36 to 0.41 and `pyo3` from 0.22 to 0.29,
  clearing RUSTSEC-2025-0020 and RUSTSEC-2026-0177 for SpecLance's own
  usage. RUSTSEC-2026-0194 and RUSTSEC-2026-0195 are still ignored in
  `audit.yml`: `lance` pulls `quick-xml` 0.26 transitively (via `pprof` ->
  `inferno`), and no upstream release clears it yet.

### Documentation

- README and docs no longer overstate what works. Added Status and Known
  limitations sections (lossy mzML export, no MS-Numpress, mzML
  `scan_num` is index + 1, no m/z index, Python returns dicts), listed
  all six vendor formats with the `all-vendors` feature, fixed the docs
  quickstart commands, and marked the design section as design intent.
- Corrected the 0.2.0-alpha entry below, which overstated what shipped.
- Added a Python API reference page (`guide/python-api`) to the docs
  site, covering the `speclance` package's `Store` class (`open`,
  `ingest_mzml`, `create_default_indexes`, `runs`, `query_window`,
  `chromatograms`). (#2, contributed by @Nabejo)

## [0.2.0-alpha] - 2026-05-31

First public alpha. SpecLance moves from `develop` to `main` as the
default branch and the workspace is ready for publication on crates.io
and PyPI.

### Added

- `speclance-core` crate: Lance-backed `Store` with shared `runs`,
  `spectra`, and `chromatograms` tables, scalar indexes on run, scan,
  retention time, and MS level, and a retention-time window query.
- `speclance-ms` crate: streaming `MzmlIngest` cursor, mzML 1.1.0
  reader, a spectra-only mzML writer (not a lossless roundtrip), and a
  `vendors` feature
  family (`thermo`, `bruker`, `waters`, `all-vendors`) that turns on
  the matching `openproteo-io` feature set.
- `speclance-cli` crate: ingestion dispatch by file extension
  (`.raw` file -> Thermo, `.raw/` dir -> Waters, `.d/` dir -> Bruker,
  `.mzML` -> mzML), Lance store management, and `speclance export` to
  emit mzML back out of a store.
- `speclance-py` crate: PyO3 bindings (`_speclance`) exposing `Store`,
  mzML ingest, indexing, and range-query, returning plain dicts.
- CI: cross-platform (ubuntu / macos / windows) Rust workflow plus a
  dedicated Python wheel job.
- `SECURITY.md`, `CONTRIBUTING.md`, `CHANGELOG.md`.
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
