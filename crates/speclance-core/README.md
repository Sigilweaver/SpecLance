# speclance-core

Core library for [SpecLance](https://github.com/Sigilweaver/SpecLance), a
columnar, memory-mapped mass spectrometry data store built on Lance. This
crate defines the on-disk Arrow schemas, the in-memory record types used
during ingest (runs, spectra, precursors, chromatograms), and the `Store`
handle for reading and writing a SpecLance directory. Most users want the
`speclance` command-line tool or the `speclance` Python package; see the
[SpecLance repository](https://github.com/Sigilweaver/SpecLance) for
installation and usage.

Licensed under Apache-2.0.
